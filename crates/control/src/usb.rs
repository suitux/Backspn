//! Access to the system's USB device list.

use std::sync::{Mutex, PoisonError};

static DEVICE_LIST: Mutex<()> = Mutex::new(());

/// Runs `walk` while no other thread walks the USB device list.
///
/// On Windows, libusb adds a device to its shared list before it fills in
/// the device's identity, so a second thread listing devices at the same
/// moment can read a half-built entry and crash the process. Every call
/// that lists or looks up devices goes through here; the list it returns
/// should be dropped inside `walk` as well.
pub(crate) fn exclusive<T>(walk: impl FnOnce() -> T) -> T {
    let _guard = DEVICE_LIST.lock().unwrap_or_else(PoisonError::into_inner);
    walk()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn walks_never_overlap() {
        let inside = AtomicUsize::new(0);
        let most = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    for _ in 0..50 {
                        exclusive(|| {
                            let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                            most.fetch_max(now, Ordering::SeqCst);
                            std::thread::yield_now();
                            inside.fetch_sub(1, Ordering::SeqCst);
                        });
                    }
                });
            }
        });
        assert_eq!(most.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_panic_inside_does_not_block_the_next_walk() {
        drop(std::panic::catch_unwind(|| {
            exclusive(|| panic!("walk failed"))
        }));
        assert_eq!(exclusive(|| 7), 7);
    }
}
