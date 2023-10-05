use tokio::runtime::{Builder, Runtime};

use crate::tracing_subscriber::subscriber::{
    set_subscriber, Config as SubscriberConfig, SubscriberManagerGuard,
};

use tracing::subscriber::SetGlobalDefaultError;

use super::{ShutdownResult, StartResult};

#[derive(thiserror::Error, Debug)]
#[allow(variant_size_differences)]
pub(crate) enum StartError {
    #[error("failed to build subscriber: {0}")]
    SubscriberBuild(#[from] crate::tracing_subscriber::subscriber::BuildError),

    #[error("failed to set global default tracing subscriber: {0}")]
    SetSubscriber(#[from] SetGlobalDefaultError),
    #[error("exporter initialization timed out: {0}")]
    ExportInitializationTimeout(#[from] tokio::time::error::Elapsed),
    #[error("failed to receive export initialization signal: {0}")]
    ExportInitializationRecv(#[from] tokio::sync::oneshot::error::RecvError),
    #[error("failed to initialize export background tokio runtime: {0}")]
    RuntimeInitialization(#[from] std::io::Error),
}

pub(crate) struct ExportProcess {
    runtime: Runtime,
    guard: SubscriberManagerGuard,
}

impl ExportProcess {
    pub(super) fn new(guard: SubscriberManagerGuard, runtime: Runtime) -> Self {
        Self { runtime, guard }
    }

    pub(super) fn start(
        subscriber_config: Box<dyn SubscriberConfig>,
        global: bool,
    ) -> StartResult<Self> {
        let runtime = init_runtime()?;
        let subscriber = runtime
            .block_on(async move { subscriber_config.build(true).map_err(StartError::from) })?;
        let guard = set_subscriber(subscriber, global)?;
        Ok(Self::new(guard, runtime))
    }

    pub(super) async fn shutdown(self) -> ShutdownResult<Runtime> {
        self.guard.shutdown().await?;
        Ok(self.runtime)
    }
}

fn init_runtime() -> Result<Runtime, StartError> {
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(StartError::RuntimeInitialization)
}
