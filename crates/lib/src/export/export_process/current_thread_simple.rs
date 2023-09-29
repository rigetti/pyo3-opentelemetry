use tracing::subscriber::DefaultGuard;

use crate::export::subscriber::SubscriberWithShutdown;

use super::ShutdownResult;

pub struct ExportProcess {
    subscriber: SubscriberWithShutdown,
    guard: Option<DefaultGuard>,
}

impl ExportProcess {
    pub(crate) fn new(subscriber: SubscriberWithShutdown) -> Self {
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
