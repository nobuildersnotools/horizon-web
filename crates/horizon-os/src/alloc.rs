//! The browser's heap. std's allocator on the web is one dlmalloc behind one
//! global lock that spins (the page may not wait), so every allocation of
//! every thread queued on the same word: with the chunk workers generating
//! and meshing, a sixth of their time went to spinning there. Here each
//! thread keeps the small blocks it freed (sizes up to [`MAX_SMALL`], in
//! size classes) and takes them back without the lock; it refills and
//! returns them in batches, one lock for many blocks. Larger blocks go to the
//! shared dlmalloc as before. Native builds use mimalloc, which does the same.

use std::alloc::{GlobalAlloc, Layout};
use std::cell::{Cell, UnsafeCell};
use std::ptr;
use std::sync::atomic::{AtomicI32, Ordering};

use crate::size_classes::{CLASSES, MAX_SMALL, class_of, class_size};

/// dlmalloc's own alignment on wasm32: the cached blocks have it.
const ALIGN: usize = 8;
/// The bytes a thread keeps of one class before it returns half.
const KEEP_BYTES: usize = 16 * 1024;
/// The bytes a refill takes from the shared heap at once.
const REFILL_BYTES: usize = 4 * 1024;

#[inline]
fn keep(class: usize) -> u32 {
    (KEEP_BYTES / class_size(class)).clamp(4, 128) as u32
}

#[inline]
fn refill(class: usize) -> u32 {
    (REFILL_BYTES / class_size(class)).clamp(1, 16) as u32
}

#[inline]
fn small(layout: Layout) -> Option<usize> {
    (layout.size() <= MAX_SMALL && layout.align() <= ALIGN).then(|| class_of(layout.size()))
}

/// The shared heap: dlmalloc behind a lock. The page's thread can't wait,
/// so it spins; a worker spins briefly, then sleeps until the holder wakes it.
/// Spinning alone fell apart when the holder was preempted: every other
/// thread then spun until it ran again.
struct Shared {
    /// 0 free, 1 held, 2 held with sleepers to wake.
    state: AtomicI32,
    heap: UnsafeCell<dlmalloc::Dlmalloc>,
}

// SAFETY: `heap` is only reached while the lock is held (`Shared::with`).
unsafe impl Sync for Shared {}

static SHARED: Shared = Shared {
    state: AtomicI32::new(0),
    heap: UnsafeCell::new(dlmalloc::Dlmalloc::new()),
};

impl Shared {
    #[inline]
    fn with<R>(&self, f: impl FnOnce(&mut dlmalloc::Dlmalloc) -> R) -> R {
        if self
            .state
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            self.lock_contended();
        }
        // SAFETY: the lock is held until the end of this call.
        let r = f(unsafe { &mut *self.heap.get() });
        if self.state.swap(0, Ordering::Release) == 2 {
            // SAFETY: the address of a live i32 in shared memory.
            unsafe { core::arch::wasm32::memory_atomic_notify(self.state.as_ptr(), 1) };
        }
        r
    }

    #[cold]
    fn lock_contended(&self) {
        let can_wait = can_wait();
        let mut spins = 0u32;
        loop {
            if self.state.load(Ordering::Relaxed) == 0
                && self
                    .state
                    .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
            {
                return;
            }
            if !can_wait || spins < 64 {
                spins += 1;
                std::hint::spin_loop();
                continue;
            }
            // Sleep, marking that the holder must wake someone; whoever slept
            // takes the lock the same way, so later sleepers are woken too.
            loop {
                if self.state.swap(2, Ordering::Acquire) == 0 {
                    return;
                }
                // SAFETY: the address of a live i32 in shared memory; a worker may wait.
                unsafe { core::arch::wasm32::memory_atomic_wait32(self.state.as_ptr(), 2, -1) };
            }
        }
    }
}

#[thread_local]
static WORKER: Cell<bool> = Cell::new(false);

/// Marks this thread as a worker, whose waits on the heap's lock may sleep
/// (`workers::horizon_worker_entry`). The page's thread never may.
pub(crate) fn this_thread_may_wait() {
    if tls_ready(&WORKER) {
        WORKER.set(true);
    }
}

#[inline]
fn can_wait() -> bool {
    tls_ready(&WORKER) && WORKER.get()
}

/// Whether this thread's thread-local storage is set up. wasm-bindgen
/// allocates a new thread's stack and its thread-local block with `malloc`
/// before that: then `__tls_base` is still 0 and a thread-local's address is
/// only its offset in the block, far below the heap (the page's 8 MiB stack
/// and the data come first). Taking the address reads no memory.
#[inline]
fn tls_ready<T>(local: &T) -> bool {
    (local as *const T as usize) >= 1 << 20
}

/// A thread's kept blocks: one list per class, linked through the blocks'
/// first word.
struct Cache {
    heads: [Cell<*mut u8>; CLASSES],
    counts: [Cell<u32>; CLASSES],
}

#[thread_local]
static CACHE: Cache = Cache {
    heads: [const { Cell::new(ptr::null_mut()) }; CLASSES],
    counts: [const { Cell::new(0) }; CLASSES],
};

/// This thread's cache, once its thread-local storage is set up (`tls_ready`).
#[inline]
fn with_cache<R>(f: impl FnOnce(Option<&Cache>) -> R) -> R {
    let cache = &CACHE;
    f(tls_ready(cache).then_some(cache))
}

impl Cache {
    #[inline]
    fn pop(&self, class: usize) -> *mut u8 {
        let head = self.heads[class].get();
        if !head.is_null() {
            // SAFETY: a kept block's first word links the next one.
            self.heads[class].set(unsafe { *(head as *mut *mut u8) });
            self.counts[class].set(self.counts[class].get() - 1);
            return head;
        }
        let size = class_size(class);
        let n = refill(class);
        SHARED.with(|heap| {
            let mut first = ptr::null_mut::<u8>();
            for _ in 0..n {
                // SAFETY: a nonzero size and dlmalloc's own alignment.
                let block = unsafe { heap.malloc(size, ALIGN) };
                if block.is_null() {
                    break;
                }
                if first.is_null() {
                    first = block;
                } else {
                    // SAFETY: the block is ours and at least a word long.
                    unsafe { *(block as *mut *mut u8) = self.heads[class].get() };
                    self.heads[class].set(block);
                    self.counts[class].set(self.counts[class].get() + 1);
                }
            }
            first
        })
    }

    #[inline]
    fn push(&self, class: usize, block: *mut u8) {
        // SAFETY: the freed block is ours and at least a word long.
        unsafe { *(block as *mut *mut u8) = self.heads[class].get() };
        self.heads[class].set(block);
        let count = self.counts[class].get() + 1;
        self.counts[class].set(count);
        if count > keep(class) {
            self.release(class, count / 2);
        }
    }

    /// Returns `n` of a class's kept blocks to the shared heap.
    fn release(&self, class: usize, n: u32) {
        let size = class_size(class);
        SHARED.with(|heap| {
            for _ in 0..n {
                let head = self.heads[class].get();
                if head.is_null() {
                    break;
                }
                // SAFETY: a kept block links the next one, and came from this heap with this size.
                unsafe {
                    self.heads[class].set(*(head as *mut *mut u8));
                    heap.free(head, size, ALIGN);
                }
                self.counts[class].set(self.counts[class].get() - 1);
            }
        });
    }
}

/// Returns every block this thread keeps to the shared heap: a worker calls
/// it when its thread ends, or they would stay with the finished thread.
pub fn release_thread_cache() {
    with_cache(|cache| {
        if let Some(cache) = cache {
            for class in 0..CLASSES {
                cache.release(class, u32::MAX);
            }
        }
    })
}

/// The game's allocator in the browser (`horizon_threads::Allocator`).
pub struct Heap;

// SAFETY: every block is dlmalloc's. A small layout's block always has its
// class's size, whether a thread's cache or the shared heap hands it out, so
// kept blocks are interchangeable; larger blocks go to dlmalloc as they are.
unsafe impl GlobalAlloc for Heap {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match small(layout) {
            Some(class) => with_cache(|cache| match cache {
                Some(cache) => cache.pop(class),
                // SAFETY: a nonzero size and dlmalloc's own alignment.
                None => SHARED.with(|heap| unsafe { heap.malloc(class_size(class), ALIGN) }),
            }),
            // SAFETY: the caller's layout.
            None => SHARED.with(|heap| unsafe { heap.malloc(layout.size(), layout.align()) }),
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        match small(layout) {
            Some(_) => {
                // SAFETY: the caller's layout.
                let block = unsafe { self.alloc(layout) };
                if !block.is_null() {
                    // SAFETY: the block holds at least `layout.size()` bytes.
                    unsafe { ptr::write_bytes(block, 0, layout.size()) };
                }
                block
            }
            // SAFETY: the caller's layout.
            None => SHARED.with(|heap| unsafe { heap.calloc(layout.size(), layout.align()) }),
        }
    }

    #[inline]
    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        match small(layout) {
            Some(class) => with_cache(|cache| match cache {
                Some(cache) => cache.push(class, block),
                // SAFETY: allocated with this class's size (`alloc`).
                None => SHARED.with(|heap| unsafe { heap.free(block, class_size(class), ALIGN) }),
            }),
            // SAFETY: allocated by dlmalloc with this layout.
            None => SHARED.with(|heap| unsafe { heap.free(block, layout.size(), layout.align()) }),
        }
    }

    #[inline]
    unsafe fn realloc(&self, block: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old = small(layout);
        // SAFETY: the caller guarantees a valid layout for the new size.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        let new = small(new_layout);
        match (old, new) {
            (Some(a), Some(b)) if a == b => block,
            (None, None) => SHARED.with(|heap| {
                // SAFETY: allocated by dlmalloc with this layout.
                unsafe { heap.realloc(block, layout.size(), layout.align(), new_size) }
            }),
            _ => {
                // SAFETY: the caller's layouts; the copy fits both blocks.
                unsafe {
                    let moved = self.alloc(new_layout);
                    if !moved.is_null() {
                        ptr::copy_nonoverlapping(block, moved, layout.size().min(new_size));
                        self.dealloc(block, layout);
                    }
                    moved
                }
            }
        }
    }
}
