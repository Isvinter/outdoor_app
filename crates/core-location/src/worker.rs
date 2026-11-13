use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::LocationError;

/// Background thread of a source with a stop signal.
pub(crate) struct Worker {
    stop: Sender<()>,
    handle: JoinHandle<()>,
}

/// Handed to the thread: lets it wait without polling and notice a stop request.
pub(crate) struct StopSignal(Receiver<()>);

impl StopSignal {
    /// Sleeps for `duration`. Returns `true` if the source was stopped meanwhile.
    pub(crate) fn wait(&self, duration: Duration) -> bool {
        !matches!(
            self.0.recv_timeout(duration),
            Err(RecvTimeoutError::Timeout)
        )
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.wait(Duration::ZERO)
    }
}

impl Worker {
    pub(crate) fn spawn(
        name: &str,
        body: impl FnOnce(StopSignal) + Send + 'static,
    ) -> Result<Self, LocationError> {
        let (stop, signal) = channel();
        let handle = std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || body(StopSignal(signal)))
            .map_err(|e| LocationError::Start(e.to_string()))?;
        Ok(Self { stop, handle })
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    /// Signals the thread to stop and waits for it to end.
    pub(crate) fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.handle.join();
    }
}

/// Ensures no worker is running before a new one is started.
pub(crate) fn ensure_idle(slot: &mut Option<Worker>) -> Result<(), LocationError> {
    match slot.take() {
        Some(worker) if !worker.is_finished() => {
            *slot = Some(worker);
            Err(LocationError::AlreadyRunning)
        }
        Some(worker) => {
            worker.stop();
            Ok(())
        }
        None => Ok(()),
    }
}
