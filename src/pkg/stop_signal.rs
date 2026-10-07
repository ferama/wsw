use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// A cloneable, one-shot stop flag that can be waited on with a timeout.
///
/// Used to make every sleep of the supervisor loop interruptible, so that a
/// stop request from the SCM is handled immediately.
#[derive(Clone, Default)]
pub struct StopSignal {
    inner: Arc<(Mutex<bool>, Condvar)>,
}

impl StopSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trigger(&self) {
        let (lock, cvar) = &*self.inner;
        let mut stopped = lock.lock().unwrap_or_else(|e| e.into_inner());
        *stopped = true;
        cvar.notify_all();
    }

    pub fn is_triggered(&self) -> bool {
        let (lock, _) = &*self.inner;
        *lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Waits until the signal is triggered or the timeout elapses.
    /// Returns true if the signal has been triggered.
    pub fn wait_timeout(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let (lock, cvar) = &*self.inner;
        let mut stopped = lock.lock().unwrap_or_else(|e| e.into_inner());
        while !*stopped {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            stopped = match cvar.wait_timeout(stopped, deadline - now) {
                Ok((guard, _)) => guard,
                Err(e) => e.into_inner().0,
            };
        }
        *stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn wait_times_out_when_not_triggered() {
        let signal = StopSignal::new();
        let start = Instant::now();
        assert!(!signal.wait_timeout(Duration::from_millis(50)));
        assert!(start.elapsed() >= Duration::from_millis(50));
        assert!(!signal.is_triggered());
    }

    #[test]
    fn trigger_wakes_up_waiters() {
        let signal = StopSignal::new();
        let remote = signal.clone();
        let handle = thread::spawn(move || remote.wait_timeout(Duration::from_secs(10)));
        thread::sleep(Duration::from_millis(20));
        signal.trigger();
        assert!(handle.join().unwrap());
        assert!(signal.is_triggered());
        // Once triggered, waits return immediately
        assert!(signal.wait_timeout(Duration::from_secs(10)));
    }
}
