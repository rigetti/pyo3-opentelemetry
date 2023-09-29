use crate::export::subscriber::SubscriberWithShutdown;

use super::{ShutdownResult, StartResult};

pub struct ExportProcess {
    subscriber: SubscriberWithShutdown,
}

impl ExportProcess {
    pub(crate) fn new(subscriber: SubscriberWithShutdown) -> Self {
        Self { subscriber }
    }

    pub(crate) fn start_tracer(&mut self) -> StartResult<()> {
        let subscriber = self.subscriber.subscriber.take().unwrap();
        tracing::subscriber::set_global_default(subscriber)?;
        Ok(())
    }

    pub(crate) async fn shutdown(self) -> ShutdownResult<()> {
        let shutdown = self.subscriber.shutdown;
        Ok(shutdown().await?)
    }
}
