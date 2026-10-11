//! One-shot access to a newly rendered KMS/FemtoVG frame before EGL swaps it.
//! No pixel storage or readback unless explicitly requested. UI thread only.
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

type Callback = Box<dyn FnOnce(Result<(), &'static str>)>;
struct Pending {
    id: u64,
    deadline: Instant,
    callback: Callback,
}

#[derive(Default)]
pub(crate) struct Session {
    next: Cell<u64>,
    pending: RefCell<Option<Pending>>,
}

thread_local! { static ACTIVE: RefCell<Weak<Session>> = RefCell::default(); }

/// Cancellation token. Cancelling an old request cannot affect a newer one.
pub struct Request {
    session: Weak<Session>,
    id: u64,
}

impl Request {
    /// Complete an outstanding request with an error (for example on timeout).
    pub fn cancel(self) {
        let Some(session) = self.session.upgrade() else {
            return;
        };
        let pending = {
            let mut slot = session.pending.borrow_mut();
            if slot.as_ref().is_some_and(|p| p.id == self.id) {
                slot.take()
            } else {
                None
            }
        };
        if let Some(pending) = pending {
            (pending.callback)(Err("timed out waiting for a newly rendered GPU frame"));
        }
    }
}

/// True only while this thread owns a live KMS/FemtoVG window context.
pub fn available() -> bool {
    ACTIVE.with_borrow(|session| session.strong_count() != 0)
}

/// Run once after the next frame's commands/notifier complete, before its swap.
/// The callback may read pixels, but must not recursively render or change UI.
/// Callers request a window redraw and schedule `Request::cancel` at `timeout`.
/// A single pending callback is allowed; rejected requests never replace it.
pub fn request(
    timeout: Duration,
    callback: impl FnOnce(Result<(), &'static str>) + 'static,
) -> Result<Request, &'static str> {
    let session = ACTIVE
        .with_borrow(Weak::upgrade)
        .ok_or("fresh frame capture requires an active KMS/FemtoVG context")?;
    session.queue(timeout, Box::new(callback))
}

impl Session {
    pub(crate) fn activate() -> Rc<Self> {
        let session = Rc::new(Self::default());
        ACTIVE.with_borrow_mut(|active| *active = Rc::downgrade(&session));
        session
    }

    fn queue(
        self: &Rc<Self>,
        timeout: Duration,
        callback: Callback,
    ) -> Result<Request, &'static str> {
        let mut slot = self.pending.borrow_mut();
        if slot.is_some() {
            return Err("a GPU screenshot request is already pending");
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or("invalid capture timeout")?;
        let id = self
            .next
            .get()
            .checked_add(1)
            .ok_or("capture request counter exhausted")?;
        self.next.set(id);
        *slot = Some(Pending {
            id,
            deadline,
            callback,
        });
        Ok(Request {
            session: Rc::downgrade(self),
            id,
        })
    }

    pub(crate) fn before_swap(&self) {
        let pending = self.pending.borrow_mut().take();
        if let Some(pending) = pending {
            let result = if Instant::now() < pending.deadline {
                Ok(())
            } else {
                Err("timed out waiting for a newly rendered GPU frame")
            };
            (pending.callback)(result);
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(pending) = self.pending.get_mut().take() {
            (pending.callback)(Err("GPU context closed before screenshot capture"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_is_once_and_precedes_swap_history_for_two_or_three_buffers() {
        for buffers in [2, 3] {
            let session = Rc::new(Session::default());
            let pixels = Rc::new(RefCell::new(vec![0; buffers]));
            let current = Rc::new(Cell::new(0));
            let captured = Rc::new(Cell::new(0));
            for frame in 1..=9 {
                let (source, slot, result) = (pixels.clone(), current.clone(), captured.clone());
                session
                    .queue(
                        Duration::from_secs(1),
                        Box::new(move |status| {
                            status.unwrap();
                            result.set(source.borrow()[slot.get()]);
                        }),
                    )
                    .unwrap();
                pixels.borrow_mut()[current.get()] = frame;
                session.before_swap();
                current.set((current.get() + 1) % buffers);
                session.before_swap(); // An idle/repeated render must not replay a callback.
                assert_eq!(captured.get(), frame);
                assert_ne!(pixels.borrow()[current.get()], frame);
            }
        }
    }

    #[test]
    fn busy_cancel_and_old_deadlines_do_not_lose_or_replace_requests() {
        let session = Rc::new(Session::default());
        let completed = Rc::new(Cell::new(0));
        let count = completed.clone();
        let old = session
            .queue(
                Duration::from_secs(1),
                Box::new(move |r| {
                    r.unwrap();
                    count.set(count.get() + 1);
                }),
            )
            .unwrap();
        assert!(session
            .queue(
                Duration::from_secs(1),
                Box::new(|_| panic!("busy callback ran"))
            )
            .is_err());
        session.before_swap();
        assert_eq!(completed.get(), 1);
        let count = completed.clone();
        let next = session
            .queue(
                Duration::from_secs(1),
                Box::new(move |r| {
                    assert!(r.is_err());
                    count.set(count.get() + 1);
                }),
            )
            .unwrap();
        old.cancel();
        assert!(session.pending.borrow().is_some());
        next.cancel();
        session.before_swap();
        assert_eq!(completed.get(), 2);
    }

    #[test]
    fn expired_and_destroyed_contexts_return_errors_without_readback() {
        let failed = Rc::new(Cell::new(0));
        let session = Rc::new(Session::default());
        for timeout in [Duration::ZERO, Duration::from_secs(1)] {
            let count = failed.clone();
            session
                .queue(
                    timeout,
                    Box::new(move |r| {
                        assert!(r.is_err());
                        count.set(count.get() + 1);
                    }),
                )
                .unwrap();
            if timeout.is_zero() {
                session.before_swap();
            }
        }
        drop(session);
        assert_eq!(failed.get(), 2);
    }
}
