use tracing::subscriber::DefaultGuard;

use crate::export::subscriber::WithShutdown;

use super::ShutdownResult;

pub(crate) struct ExportProcess {
    subscriber: WithShutdown,
    guard: Option<DefaultGuard>,
}

impl ExportProcess {
    pub(crate) const fn new(subscriber: WithShutdown) -> Self {
        Self {
            subscriber,
            guard: None,
        }
    }

    pub(crate) fn start_tracer(&mut self) {
        let subscriber = self.subscriber.subscriber.take().unwrap();
        self.guard = Some(tracing::subscriber::set_default(subscriber));
    }

    pub(crate) async fn shutdown(mut self) -> ShutdownResult<()> {
        self.guard.take();
        let shutdown = self.subscriber.shutdown;
        shutdown().await?;
        Ok(())
    }
}
