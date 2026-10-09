//! One long-lived clipboard per process.
//!
//! On X11 and Wayland the program that last set the clipboard serves its contents to whoever
//! pastes, so a clipboard dropped as soon as the call returns takes the text with it. The owner
//! is made on first use and kept until the process ends.

use std::sync::Mutex;

/// A lazily made value that lives as long as the owner does.
pub(crate) struct Owner<C> {
    slot: Mutex<Option<C>>,
}

impl<C> Owner<C> {
    pub(crate) const fn new() -> Self {
        Self {
            slot: Mutex::new(None),
        }
    }

    /// Run `f` on the held value, making it with `make` the first time. `None` when it cannot
    /// be made (a later call tries again).
    pub(crate) fn with<T>(
        &self,
        make: impl FnOnce() -> Option<C>,
        f: impl FnOnce(&mut C) -> T,
    ) -> Option<T> {
        let mut slot = self
            .slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_none() {
            *slot = make();
        }
        slot.as_mut().map(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Probe(Arc<AtomicUsize>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn the_held_value_outlives_the_call_and_is_made_once() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let made = AtomicUsize::new(0);
        let owner = Owner::new();
        for _ in 0..3 {
            owner.with(
                || {
                    made.fetch_add(1, Ordering::SeqCst);
                    Some(Probe(dropped.clone()))
                },
                |_| (),
            );
            assert_eq!(
                dropped.load(Ordering::SeqCst),
                0,
                "dropped when the call returned"
            );
        }
        assert_eq!(made.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_failed_make_is_tried_again() {
        let owner: Owner<u8> = Owner::new();
        assert_eq!(owner.with(|| None, |_| ()), None);
        assert_eq!(owner.with(|| Some(7), |v| *v), Some(7));
    }
}
