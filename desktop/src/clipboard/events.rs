// SPDX-License-Identifier: GPL-3.0-or-later

//! Native clipboard change events, read on a thread of their own. The clipboard thread asks
//! [`ChangeEvents::take_change`] at each poll and reads the clipboard only when it says so, the
//! way Windows' sequence number is used.
//!
//! If the events stop (the compositor went away, a protocol error, even a panic), every poll
//! reads again, as without events.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;

use anyhow::{Context, Result};

/// Blocks until the clipboard changes. Returns `Ok(false)` once stopped.
pub type NextChange = Box<dyn FnMut() -> Result<bool>>;
/// Makes a blocked [`NextChange`] return `Ok(false)`. Called from another thread.
pub type Stop = Box<dyn Fn() + Send>;

/// The events thread. Dropping this stops it.
pub struct ChangeEvents {
    state: Arc<State>,
    stop: Stop,
}

struct State {
    changed: AtomicBool,
    running: AtomicBool,
}

impl ChangeEvents {
    /// Starts the thread. `connect` runs on it: it returns how to wait for a change and how to
    /// stop waiting. If `connect` fails, so does this, and the thread is gone.
    pub fn start(
        connect: impl FnOnce() -> Result<(NextChange, Stop)> + Send + 'static,
    ) -> Result<Self> {
        let state = Arc::new(State {
            // The first poll reads, as it would without events.
            changed: AtomicBool::new(true),
            running: AtomicBool::new(true),
        });
        let (connected_tx, connected) = mpsc::sync_channel(1);
        let thread_state = Arc::clone(&state);
        thread::Builder::new()
            .name("clipboard-events".into())
            .spawn(move || {
                let _ended = Ended(Arc::clone(&thread_state));
                let mut next_change = match connect() {
                    Ok((next_change, stop)) => {
                        let _ = connected_tx.send(Ok(stop));
                        next_change
                    }
                    Err(e) => {
                        let _ = connected_tx.send(Err(e));
                        return;
                    }
                };
                loop {
                    match next_change() {
                        Ok(true) => thread_state.changed.store(true, Ordering::SeqCst),
                        Ok(false) => return,
                        Err(e) => {
                            tracing::warn!(
                                error = %format!("{e:#}"),
                                "clipboard change events stopped: checking the clipboard at every poll"
                            );
                            return;
                        }
                    }
                }
            })
            .context("could not start the clipboard events thread")?;
        // A panic in `connect` drops the sender: an error here, not a hang.
        let stop = connected
            .recv()
            .context("the clipboard events thread ended")??;
        Ok(Self { state, stop })
    }

    /// Whether the clipboard may have changed since the last call. Always `true` once the events
    /// have stopped.
    pub fn take_change(&self) -> bool {
        self.state.changed.swap(false, Ordering::SeqCst)
            || !self.state.running.load(Ordering::SeqCst)
    }
}

impl Drop for ChangeEvents {
    fn drop(&mut self) {
        (self.stop)();
    }
}

/// Marks the events as stopped when the thread ends, whether it returns or panics.
struct Ended(Arc<State>);

impl Drop for Ended {
    fn drop(&mut self) {
        self.0.running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    enum Fake {
        Change,
        Fail,
        Panic,
        Stop,
    }

    /// Drives a fake event source. `ended` disconnects once the events thread has ended.
    struct FakeSource {
        events: mpsc::Sender<Fake>,
        ended: mpsc::Receiver<()>,
    }

    fn start_fake() -> (ChangeEvents, FakeSource) {
        let (events, received) = mpsc::channel();
        let (alive, ended) = mpsc::channel::<()>();
        let stop_sender = events.clone();
        let started = ChangeEvents::start(move || {
            let next_change: NextChange = Box::new(move || {
                let _alive = &alive;
                match received.recv() {
                    Ok(Fake::Change) => Ok(true),
                    Ok(Fake::Fail) => anyhow::bail!("the compositor went away"),
                    Ok(Fake::Panic) => panic!("a bug in the event source"),
                    Ok(Fake::Stop) | Err(_) => Ok(false),
                }
            });
            let stop: Stop = Box::new(move || {
                let _ = stop_sender.send(Fake::Stop);
            });
            Ok((next_change, stop))
        });
        (started.unwrap(), FakeSource { events, ended })
    }

    fn eventually(mut condition: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if condition() {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        false
    }

    fn has_ended(source: &FakeSource) -> bool {
        source.ended.recv_timeout(Duration::from_secs(2))
            == Err(mpsc::RecvTimeoutError::Disconnected)
    }

    #[test]
    fn a_change_is_reported_once() {
        let (events, source) = start_fake();
        assert!(events.take_change(), "the first poll should read");
        assert!(!events.take_change());

        source.events.send(Fake::Change).unwrap();

        assert!(eventually(|| events.take_change()));
        assert!(!events.take_change());
    }

    #[test]
    fn dropping_stops_the_thread() {
        let (events, source) = start_fake();

        drop(events);

        assert!(has_ended(&source));
    }

    #[test]
    fn a_compositor_without_the_protocol_is_an_error() {
        let result = ChangeEvents::start(|| anyhow::bail!("no data-control protocol"));

        assert!(result.is_err());
    }

    #[test]
    fn a_panic_while_connecting_is_an_error() {
        let result = ChangeEvents::start(|| panic!("a bug while connecting"));

        assert!(result.is_err());
    }

    #[test]
    fn every_poll_reads_once_the_events_fail() {
        let (events, source) = start_fake();
        events.take_change();

        source.events.send(Fake::Fail).unwrap();

        assert!(has_ended(&source));
        assert!(eventually(|| events.take_change() && events.take_change()));
    }

    #[test]
    fn every_poll_reads_once_the_events_panic() {
        let (events, source) = start_fake();
        events.take_change();

        source.events.send(Fake::Panic).unwrap();

        assert!(has_ended(&source));
        assert!(eventually(|| events.take_change() && events.take_change()));
    }
}
