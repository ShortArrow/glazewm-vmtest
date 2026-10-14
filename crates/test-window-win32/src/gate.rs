//! A gate that holds a window's thread, so that the window stops processing messages until it is opened.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Closed, it holds the window's thread that reaches it; opened, it lets that thread go on.
#[derive(Default)]
pub struct Gate {
    state: Mutex<GateState>,
    changed: Condvar,
}

#[derive(Default)]
struct GateState {
    closed: bool,
    holding: bool,
}

impl Gate {
    /// Makes the next `hold_while_closed` hold its thread.
    pub fn close(&self) {
        self.update(|state| state.closed = true);
    }

    /// Releases the thread held, if any.
    pub fn open(&self) {
        self.update(|state| state.closed = false);
    }

    /// Waits until a thread is held, and says whether one was within `limit`.
    pub fn wait_until_holding(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        let mut state = self.lock();
        while !state.holding {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
        true
    }

    /// Called on the window's thread: returns at once while open, and otherwise once opened.
    pub fn hold_while_closed(&self) {
        let mut state = self.lock();
        state.holding = true;
        self.changed.notify_all();
        while state.closed {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state.holding = false;
        self.changed.notify_all();
    }

    fn update(&self, change: impl FnOnce(&mut GateState)) {
        change(&mut self.lock());
        self.changed.notify_all();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, GateState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    const LIMIT: Duration = Duration::from_secs(2);

    #[test]
    fn holds_the_window_thread_from_closing_until_opening() {
        let gate = Arc::new(Gate::default());
        let released = Arc::new(AtomicBool::new(false));
        gate.close();
        let thread = {
            let gate = Arc::clone(&gate);
            let released = Arc::clone(&released);
            std::thread::spawn(move || {
                gate.hold_while_closed();
                released.store(true, Ordering::SeqCst);
            })
        };

        assert!(gate.wait_until_holding(LIMIT));
        std::thread::sleep(Duration::from_millis(50));
        assert!(!released.load(Ordering::SeqCst));
        gate.open();
        thread.join().unwrap();
        assert!(released.load(Ordering::SeqCst));
    }

    #[test]
    fn reports_a_thread_that_never_arrives_after_the_limit() {
        let gate = Gate::default();
        gate.close();

        assert!(!gate.wait_until_holding(Duration::from_millis(50)));
    }

    #[test]
    fn lets_the_thread_through_while_open() {
        Gate::default().hold_while_closed();
    }
}
