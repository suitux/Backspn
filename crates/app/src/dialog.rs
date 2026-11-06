//! Native file dialogs that never block a frame.
//!
//! A file dialog shown from inside a frame runs an event loop of its own
//! while the window's event is still being handled; on macOS the window then
//! receives a nested event and the app aborts. Each dialog here runs on a
//! thread of its own instead, and a later frame picks the answer up.
//!
//! Only one dialog is open at a time: asking for another one while a dialog
//! is open does nothing.

use std::any::Any;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use eframe::egui::{self, Id};

type Answer = Box<dyn Any + Send>;
type Handler<S> = Box<dyn FnOnce(&mut S, Answer)>;

enum Closed {
    Picked(Answer),
    Cancelled,
}

struct State {
    open: bool,
    done: Option<(Id, Closed)>,
}

pub struct Dialogs {
    state: Mutex<State>,
    repaint: OnceLock<egui::Context>,
}

static DIALOGS: Dialogs = Dialogs::new();

impl Dialogs {
    const fn new() -> Self {
        Self {
            state: Mutex::new(State {
                open: false,
                done: None,
            }),
            repaint: OnceLock::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn open<T: Send + 'static>(
        &'static self,
        key: Id,
        pick: impl FnOnce() -> Option<T> + Send + 'static,
    ) -> bool {
        {
            let mut state = self.lock();
            if state.open {
                return false;
            }
            state.open = true;
        }
        let spawned = std::thread::Builder::new()
            .name("file dialog".into())
            .spawn(move || {
                let closing = Closing(self);
                let closed = pick().map_or(Closed::Cancelled, |t| Closed::Picked(Box::new(t)));
                self.lock().done = Some((key, closed));
                drop(closing);
                if let Some(ctx) = self.repaint.get() {
                    ctx.request_repaint();
                }
            });
        if let Err(e) = spawned {
            log::error!("cannot open the file dialog: {e}");
            self.lock().open = false;
            return false;
        }
        true
    }

    /// How the dialog opened with `key` closed, once it did.
    fn closed(&self, key: Id) -> Option<Closed> {
        let mut state = self.lock();
        if state.done.as_ref().is_some_and(|(k, _)| *k == key) {
            state.done.take().map(|(_, closed)| closed)
        } else {
            None
        }
    }

    fn take<T: 'static>(&self, key: Id) -> Option<T> {
        match self.closed(key)? {
            Closed::Picked(answer) => answer.downcast().ok().map(|t| *t),
            Closed::Cancelled => None,
        }
    }
}

/// Marks the dialog closed even when picking panics.
struct Closing(&'static Dialogs);

impl Drop for Closing {
    fn drop(&mut self) {
        self.0.lock().open = false;
    }
}

/// Lets a closed dialog wake the app up for the frame that takes its answer.
pub fn set_repaint(ctx: &egui::Context) {
    // Set once: a later call has the same context.
    let _ = DIALOGS.repaint.set(ctx.clone());
}

/// Opens `pick` when `open` is true and returns its answer in the frame it
/// is ready. Call it every frame with the same `key`, such as the id of the
/// button that opens the dialog.
pub fn ask<T: Send + 'static>(
    key: Id,
    open: bool,
    pick: impl FnOnce() -> Option<T> + Send + 'static,
) -> Option<T> {
    if open {
        DIALOGS.open(key, pick);
    }
    DIALOGS.take(key)
}

/// What an owner does with the answer of the dialog it opened, for dialogs
/// opened outside the code drawn every frame (menus, actions). The owner
/// calls [`Then::finish`] every frame.
pub struct Then<S> {
    dialogs: &'static Dialogs,
    key: Id,
    handler: Option<Handler<S>>,
}

impl<S> Then<S> {
    pub fn new(key: &str) -> Self {
        Self {
            dialogs: &DIALOGS,
            key: Id::unique(key),
            handler: None,
        }
    }

    /// Opens `pick` and, once it is answered, runs `then` with the answer.
    pub fn open<T: Send + 'static>(
        &mut self,
        pick: impl FnOnce() -> Option<T> + Send + 'static,
        then: impl FnOnce(&mut S, T) + 'static,
    ) {
        if self.dialogs.open(self.key, pick) {
            self.handler = Some(Box::new(move |owner, answer| {
                if let Ok(t) = answer.downcast::<T>() {
                    then(owner, *t);
                }
            }));
        }
    }

    pub fn finish(owner: &mut S, field: impl Fn(&mut S) -> &mut Self) {
        let me = field(owner);
        if me.handler.is_none() {
            return;
        }
        let Some(closed) = me.dialogs.closed(me.key) else {
            return;
        };
        if let (Some(handler), Closed::Picked(answer)) = (me.handler.take(), closed) {
            handler(owner, answer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn dialogs() -> &'static Dialogs {
        Box::leak(Box::new(Dialogs::new()))
    }

    fn wait_closed(d: &Dialogs) {
        let start = Instant::now();
        while d.lock().open {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "dialog never closed"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn answers_reach_the_key_that_asked() {
        let d = dialogs();
        let (a, b) = (Id::unique("a"), Id::unique("b"));
        assert!(d.open(a, || Some(7_u32)));
        wait_closed(d);
        assert_eq!(d.take::<u32>(b), None);
        assert_eq!(d.take::<u32>(a), Some(7));
        assert_eq!(d.take::<u32>(a), None);
    }

    #[test]
    fn one_dialog_at_a_time() {
        let d = dialogs();
        let (go, wait) = std::sync::mpsc::channel::<()>();
        assert!(d.open(Id::unique("a"), move || wait.recv().ok()));
        assert!(!d.open(Id::unique("b"), || Some(())));
        go.send(()).unwrap();
        wait_closed(d);
        assert!(d.open(Id::unique("b"), || Some(())));
        wait_closed(d);
    }

    #[test]
    fn a_panicking_dialog_still_closes() {
        let d = dialogs();
        assert!(d.open(Id::unique("a"), || -> Option<()> { panic!("no dialog") }));
        wait_closed(d);
        assert!(d.open(Id::unique("a"), || Some(())));
        wait_closed(d);
    }

    struct Owner {
        dialog: Then<Owner>,
        got: Vec<u32>,
    }

    fn owner(d: &'static Dialogs) -> Owner {
        Owner {
            dialog: Then {
                dialogs: d,
                key: Id::unique("owner"),
                handler: None,
            },
            got: Vec::new(),
        }
    }

    #[test]
    fn then_runs_once_with_the_answer() {
        let d = dialogs();
        let mut o = owner(d);
        o.dialog
            .open(|| Some(3_u32), |o: &mut Owner, n| o.got.push(n));
        wait_closed(d);
        Then::finish(&mut o, |o| &mut o.dialog);
        Then::finish(&mut o, |o| &mut o.dialog);
        assert_eq!(o.got, [3]);
    }

    #[test]
    fn a_cancelled_dialog_runs_nothing() {
        let d = dialogs();
        let mut o = owner(d);
        o.dialog
            .open(|| None::<u32>, |o: &mut Owner, n| o.got.push(n));
        wait_closed(d);
        Then::finish(&mut o, |o| &mut o.dialog);
        assert!(o.dialog.handler.is_none());
        assert_eq!(o.got, [0_u32; 0]);
    }
}
