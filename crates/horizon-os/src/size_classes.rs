//! The size classes of the browser's heap (`alloc.rs`): a small block has
//! the size of its class, so freed blocks of a class can be handed out again
//! for any size in it.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

/// The largest size a thread's cache keeps.
pub(crate) const MAX_SMALL: usize = 4096;
/// Sizes 16, 32 .. 256, then four classes per doubling up to [`MAX_SMALL`].
pub(crate) const CLASSES: usize = 16 + 4 * 4;
/// The class of a small size (1..=[`MAX_SMALL`]).
#[inline]
pub(crate) fn class_of(size: usize) -> usize {
    if size <= 256 {
        size.div_ceil(16) - 1
    } else {
        // (2^p, 2^(p+1)] in four steps of 2^(p-2).
        let s = size - 1;
        let p = usize::BITS as usize - 1 - s.leading_zeros() as usize;
        16 + (p - 8) * 4 + ((s - (1 << p)) >> (p - 2))
    }
}

/// The size of a class's blocks.
#[inline]
pub(crate) fn class_size(class: usize) -> usize {
    if class < 16 {
        (class + 1) * 16
    } else {
        let q = class - 16;
        let p = 8 + q / 4;
        (1 << p) + (q % 4 + 1) * (1 << (p - 2))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_small_size_fits_its_class_and_classes_grow() {
        let mut last = 0;
        for size in 1..=MAX_SMALL {
            let class = class_of(size);
            assert!(class < CLASSES, "{size}");
            assert!(class_size(class) >= size, "{size}");
            assert!(class >= last, "{size}");
            if class > 0 {
                assert!(class_size(class - 1) < size, "{size} could use a smaller class");
            }
            last = class;
        }
        assert_eq!(class_of(MAX_SMALL), CLASSES - 1);
        assert_eq!(class_size(CLASSES - 1), MAX_SMALL);
    }
}
