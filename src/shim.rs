//! `no_std` compatibility shims.
//!
//! Everything here is a drop-in for a `std` facility so that call sites in the
//! rest of the crate stay byte-identical under both configurations.

/// `OnceLock` under `std`; a single-threaded equivalent otherwise.
#[cfg(feature = "std")]
pub(crate) use std::sync::OnceLock;

#[cfg(not(feature = "std"))]
pub(crate) use nostd_once::OnceLock;

#[cfg(not(feature = "std"))]
mod nostd_once {
    use core::cell::UnsafeCell;

    /// Single-threaded stand-in for `std::sync::OnceLock`.
    pub(crate) struct OnceLock<T>(UnsafeCell<Option<T>>);

    // SAFETY: `no_std` builds of this crate are single-threaded by contract.
    unsafe impl<T> Sync for OnceLock<T> {}

    impl<T> OnceLock<T> {
        pub(crate) const fn new() -> Self {
            Self(UnsafeCell::new(None))
        }

        pub(crate) fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
            // SAFETY: single-threaded; no reentrant call reaches `f`.
            unsafe {
                let slot = &mut *self.0.get();
                if slot.is_none() {
                    *slot = Some(f());
                }
                slot.as_ref().unwrap_unchecked()
            }
        }
    }
}

/// `f64` methods that live in `std` rather than `core`.
///
/// Under `std` the inherent methods win and this trait is inert; under
/// `no_std` it routes to `libm`, so call sites need no `cfg`.
#[allow(dead_code)]
pub(crate) trait F64Ext {
    fn trunc(self) -> f64;
    fn floor(self) -> f64;
    fn ceil(self) -> f64;
    fn round(self) -> f64;
    fn log10(self) -> f64;
}

impl F64Ext for f64 {
    #[inline]
    fn trunc(self) -> f64 {
        #[cfg(feature = "std")]
        return f64::trunc(self);
        #[cfg(not(feature = "std"))]
        return libm::trunc(self);
    }
    #[inline]
    fn floor(self) -> f64 {
        #[cfg(feature = "std")]
        return f64::floor(self);
        #[cfg(not(feature = "std"))]
        return libm::floor(self);
    }
    #[inline]
    fn ceil(self) -> f64 {
        #[cfg(feature = "std")]
        return f64::ceil(self);
        #[cfg(not(feature = "std"))]
        return libm::ceil(self);
    }
    #[inline]
    fn round(self) -> f64 {
        #[cfg(feature = "std")]
        return f64::round(self);
        #[cfg(not(feature = "std"))]
        return libm::round(self);
    }
    #[inline]
    fn log10(self) -> f64 {
        #[cfg(feature = "std")]
        return f64::log10(self);
        #[cfg(not(feature = "std"))]
        return libm::log10(self);
    }
}

/// `thread_local!` with a `const` initializer under `std`; a single-threaded
/// static otherwise. Supports only the `const { .. }` form this crate uses.
#[cfg(feature = "std")]
macro_rules! thread_local_cell {
    ($(#[$m:meta])* static $name:ident: $ty:ty = const $init:block;) => {
        std::thread_local! {
            $(#[$m])* static $name: $ty = const $init;
        }
    };
}

#[cfg(not(feature = "std"))]
macro_rules! thread_local_cell {
    ($(#[$m:meta])* static $name:ident: $ty:ty = const $init:block;) => {
        $(#[$m])*
        static $name: $crate::shim::SingleThreadLocal<$ty> =
            $crate::shim::SingleThreadLocal::new($init);
    };
}

pub(crate) use thread_local_cell;

/// Single-threaded stand-in for a `thread_local!` slot, exposing the subset of
/// `LocalKey` this crate uses.
#[cfg(not(feature = "std"))]
pub(crate) struct SingleThreadLocal<T>(T);

#[cfg(not(feature = "std"))]
// SAFETY: `no_std` builds of this crate are single-threaded by contract.
unsafe impl<T> Sync for SingleThreadLocal<T> {}

#[cfg(not(feature = "std"))]
impl<T> SingleThreadLocal<core::cell::RefCell<T>> {
    pub(crate) const fn new(value: core::cell::RefCell<T>) -> Self {
        Self(value)
    }

    pub(crate) fn with_borrow_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut self.0.borrow_mut())
    }
}
