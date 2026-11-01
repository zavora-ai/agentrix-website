//! The shared circuit breaker: consecutive failures open the circuit for a cooldown,
//! one probe half-opens it, and success closes it again.
//!
//! Every live HTTP seam calls through a [`Breaker`]: transport errors and 5xx
//! responses retry with backoff while the circuit is closed, and once the
//! failures pass `threshold`, calls fail fast without touching the network until
//! the cooldown passes. Sync throughout: the live clients are blocking.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What the breaker itself reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Open {
    /// The seam that is open.
    pub seam: String,
    /// How many consecutive failures opened it.
    pub failures: u32,
}

impl std::fmt::Display for Open {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "circuit open for {} after {} failures",
            self.seam, self.failures
        )
    }
}

#[derive(Debug)]
enum State {
    Closed { failures: u32 },
    Open { since: Instant, failures: u32 },
    HalfOpen,
}

/// A circuit breaker for one seam: threshold consecutive failures open it for
/// `cooldown`, then one probe decides whether it closes.
#[derive(Debug)]
pub struct Breaker {
    seam: String,
    threshold: u32,
    cooldown: Duration,
    state: Mutex<State>,
}

impl Breaker {
    /// A breaker for `seam` opening after `threshold` consecutive failures for
    /// `cooldown` each time.
    pub fn new(seam: impl Into<String>, threshold: u32, cooldown: Duration) -> Self {
        Self {
            seam: seam.into(),
            threshold: threshold.max(1),
            cooldown,
            state: Mutex::new(State::Closed { failures: 0 }),
        }
    }

    /// The seam's name.
    pub fn seam(&self) -> &str {
        &self.seam
    }

    /// Whether a call may proceed: closed always, open only past the cooldown
    /// (promoting to half-open), half-open never — one probe at a time.
    fn admit(&self) -> Result<(), Open> {
        let mut state = self.state.lock().expect("breaker");
        match *state {
            State::Closed { .. } => Ok(()),
            State::Open { since, failures } => {
                if since.elapsed() >= self.cooldown {
                    *state = State::HalfOpen;
                    Ok(())
                } else {
                    Err(Open {
                        seam: self.seam.clone(),
                        failures,
                    })
                }
            }
            State::HalfOpen => Err(Open {
                seam: self.seam.clone(),
                failures: self.threshold,
            }),
        }
    }

    /// Records success: any state closes with a clean count.
    pub fn record_success(&self) {
        *self.state.lock().expect("breaker") = State::Closed { failures: 0 };
    }

    /// Records failure: the count rises, opening at the threshold.
    pub fn record_failure(&self) {
        let mut state = self.state.lock().expect("breaker");
        let failures = match *state {
            State::Closed { failures } => failures + 1,
            State::Open { failures, .. } => failures + 1,
            State::HalfOpen => self.threshold,
        };
        if failures >= self.threshold {
            *state = State::Open {
                since: Instant::now(),
                failures,
            };
        } else {
            *state = State::Closed { failures };
        }
    }

    /// Calls `op` through the breaker with `attempts` total tries and exponential
    /// backoff from `base` (capped at 30 seconds): retryable failures sleep and
    /// retry, terminal ones return at once, and an open circuit fails without
    /// calling. `retryable` classifies the error.
    pub fn call<T, E>(
        &self,
        attempts: u32,
        base: Duration,
        mut op: impl FnMut() -> Result<T, E>,
        retryable: impl Fn(&E) -> bool,
    ) -> Result<T, BreakerCall<E>> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            self.admit().map_err(BreakerCall::Open)?;
            match op() {
                Ok(value) => {
                    self.record_success();
                    return Ok(value);
                }
                Err(error) => {
                    self.record_failure();
                    if !retryable(&error) || attempt >= attempts.max(1) {
                        return Err(BreakerCall::Error(error));
                    }
                    let backoff = (base * 2_u32.pow(attempt - 1)).min(Duration::from_secs(30));
                    std::thread::sleep(backoff);
                }
            }
        }
    }
}

/// What a guarded call reports: the circuit open, or the operation's error.
#[derive(Debug)]
pub enum BreakerCall<E> {
    /// The circuit is open: nothing was attempted.
    Open(Open),
    /// The operation failed.
    Error(E),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_fast_and_recovers() {
        let breaker = Breaker::new("test", 2, Duration::from_millis(50));
        let failing = || -> Result<(), &str> { Err("down") };
        assert!(matches!(
            breaker.call(1, Duration::ZERO, failing, |_| true),
            Err(BreakerCall::Error(_))
        ));
        assert!(matches!(
            breaker.call(1, Duration::ZERO, failing, |_| true),
            Err(BreakerCall::Error(_))
        ));
        // Open: fails without calling.
        let mut called = false;
        let probe = || {
            called = true;
            Ok::<(), &str>(())
        };
        assert!(matches!(
            breaker.call(1, Duration::ZERO, probe, |_| true),
            Err(BreakerCall::Open(_))
        ));
        assert!(!called);
        // Past the cooldown the probe closes it.
        std::thread::sleep(Duration::from_millis(60));
        assert!(
            breaker
                .call(1, Duration::ZERO, || Ok::<(), &str>(()), |_| true)
                .is_ok()
        );
        assert!(
            breaker
                .call(1, Duration::ZERO, || Ok::<(), &str>(()), |_| true)
                .is_ok()
        );
    }

    #[test]
    fn retries_then_returns() {
        let breaker = Breaker::new("test", 10, Duration::from_secs(60));
        let mut tries = 0;
        let flaky = || {
            tries += 1;
            if tries < 3 { Err("flaky") } else { Ok(()) }
        };
        assert!(breaker.call(5, Duration::ZERO, flaky, |_| true).is_ok());
        assert_eq!(tries, 3);
        // Terminal errors return at once.
        let mut tries = 0;
        let terminal = || {
            tries += 1;
            Err::<(), &str>("fatal")
        };
        assert!(matches!(
            breaker.call(5, Duration::ZERO, terminal, |_| false),
            Err(BreakerCall::Error(_))
        ));
        assert_eq!(tries, 1);
    }
}
