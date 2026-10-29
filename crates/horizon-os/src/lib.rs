//! The operating system as the game sees it: files, clocks and the process's
//! folder. On native these are std's own; in the browser, where std has no
//! filesystem and its clocks panic, they are a virtual filesystem (backed by
//! files fetched from the page's origin) and the browser's clocks, with the
//! same API so call sites read the same on both.

pub mod env;
pub mod fs;
#[cfg(target_arch = "wasm32")]
pub mod web;

/// Waits for `future`. On native, as long as it takes; on the web, where the
/// page's only thread can't wait for the browser, `None` unless the future is
/// ready on its first poll.
pub fn block_on<F: std::future::Future>(future: F) -> Option<F::Output> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Some(pollster::block_on(future))
    }
    #[cfg(target_arch = "wasm32")]
    {
        use std::task::{Context, Poll, Waker};
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(output) => Some(output),
            Poll::Pending => None,
        }
    }
}

/// std's clocks on native; the browser's `performance.now()` and `Date.now()`
/// on the web, where `std::time::Instant::now()` panics.
pub mod time {
    pub use std::time::Duration;
    pub use web_time::{Instant, SystemTime, SystemTimeError, UNIX_EPOCH};

    /// The same moment as std's `SystemTime`, for libraries that take std's
    /// (chrono): the identity on native.
    pub fn to_std(t: SystemTime) -> std::time::SystemTime {
        std::time::UNIX_EPOCH + t.duration_since(UNIX_EPOCH).unwrap_or_default()
    }
}

/// std's threads. The page's build has no threads yet: scoped threads run
/// one after another on the caller's, and sleeping returns at once (the page's
/// thread may not block).
pub mod thread {
    pub use std::thread::*;

    #[cfg(target_arch = "wasm32")]
    pub use sequential::{Builder, Scope, ScopedJoinHandle, scope, sleep, spawn};

    #[cfg(target_arch = "wasm32")]
    mod sequential {
        use std::marker::PhantomData;

        pub struct Scope<'scope, 'env: 'scope> {
            _scope: PhantomData<&'scope mut &'scope ()>,
            _env: PhantomData<&'env mut &'env ()>,
        }

        pub struct ScopedJoinHandle<'scope, T> {
            result: T,
            _scope: PhantomData<&'scope ()>,
        }

        impl<'scope, 'env> Scope<'scope, 'env> {
            /// Runs `f` now and keeps what it returns for `join`.
            pub fn spawn<F, T>(&'scope self, f: F) -> ScopedJoinHandle<'scope, T>
            where
                F: FnOnce() -> T + Send + 'scope,
                T: Send + 'scope,
            {
                ScopedJoinHandle {
                    result: f(),
                    _scope: PhantomData,
                }
            }
        }

        impl<T> ScopedJoinHandle<'_, T> {
            pub fn join(self) -> std::thread::Result<T> {
                Ok(self.result)
            }
            pub fn is_finished(&self) -> bool {
                true
            }
        }

        pub fn scope<'env, F, T>(f: F) -> T
        where
            F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T,
        {
            f(&Scope {
                _scope: PhantomData,
                _env: PhantomData,
            })
        }

        pub fn sleep(_dur: std::time::Duration) {}

        /// std's builder, which can't start a thread on the page: says which
        /// thread was wanted before failing as std does.
        #[derive(Debug)]
        pub struct Builder {
            name: Option<String>,
        }

        impl Builder {
            pub fn new() -> Self {
                Builder { name: None }
            }
            pub fn name(mut self, name: String) -> Self {
                self.name = Some(name);
                self
            }
            pub fn stack_size(self, _size: usize) -> Self {
                self
            }
            pub fn spawn<F, T>(self, f: F) -> std::io::Result<std::thread::JoinHandle<T>>
            where
                F: FnOnce() -> T + Send + 'static,
                T: Send + 'static,
            {
                crate::web::console(
                    2,
                    &format!(
                        "Horizon: no thread on the page for {}",
                        self.name.as_deref().unwrap_or("an unnamed thread")
                    ),
                );
                std::thread::Builder::new().spawn(f)
            }
        }

        impl Default for Builder {
            fn default() -> Self {
                Self::new()
            }
        }

        pub fn spawn<F, T>(f: F) -> std::thread::JoinHandle<T>
        where
            F: FnOnce() -> T + Send + 'static,
            T: Send + 'static,
        {
            Builder::new().spawn(f).expect("failed to spawn thread")
        }
    }
}

/// The process. The page has one game and no process id: 1.
pub mod process {
    #[cfg(not(target_arch = "wasm32"))]
    pub use std::process::id;

    #[cfg(target_arch = "wasm32")]
    pub fn id() -> u32 {
        1
    }
}
