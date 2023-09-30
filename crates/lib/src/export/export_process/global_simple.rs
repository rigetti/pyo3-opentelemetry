use crate::export::subscriber::WithShutdown;

use super::{ShutdownResult, StartResult};

pub(crate) struct ExportProcess {
    subscriber: WithShutdown,
}

impl ExportProcess {
    pub(crate) const fn new(subscriber: WithShutdown) -> Self {
        Self { subscriber }
    }

    pub(crate) fn start_tracer(&mut self) -> StartResult<()> {
        let subscriber = self.subscriber.subscriber.take().unwrap();
        tracing::subscriber::set_global_default(subscriber)?;
        Ok(())
    }

    pub(crate) async fn shutdown(self) -> ShutdownResult<()> {
        let shutdown = self.subscriber.shutdown;
        shutdown().await?;
        Ok(())
    }
}
