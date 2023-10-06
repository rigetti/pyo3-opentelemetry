use tokio::runtime::{Builder, Runtime};

use crate::subscriber::{set_subscriber, Config as SubscriberConfig, SubscriberManagerGuard};

use tracing::subscriber::SetGlobalDefaultError;

use super::{ShutdownResult, StartResult};

#[derive(thiserror::Error, Debug)]
#[allow(variant_size_differences)]
pub(crate) enum StartError {
    #[error("failed to build subscriber: {0}")]
    SubscriberBuild(#[from] crate::subscriber::BuildError),
    #[error("failed to set global default tracing subscriber: {0}")]
    SetSubscriber(#[from] SetGlobalDefaultError),
    #[error("exporter initialization timed out: {0}")]
    ExportInitializationTimeout(#[from] tokio::time::error::Elapsed),
    #[error("failed to receive export initialization signal: {0}")]
    ExportInitializationRecv(#[from] tokio::sync::oneshot::error::RecvError),
    #[error("failed to initialize export background tokio runtime: {0}")]
    RuntimeInitialization(#[from] std::io::Error),
}

/// Carries the background tokio runtime and the subscriber manager guard.
pub(crate) struct ExportProcess {
    runtime: Runtime,
    guard: SubscriberManagerGuard,
}

impl ExportProcess {
    fn new(guard: SubscriberManagerGuard, runtime: Runtime) -> Self {
        Self { runtime, guard }
    }

    /// Starts a background export process. Importantly, this:
    ///
    /// * Initializes a new tokio runtime, which will be persisted within the returned `Self`.
    /// * Builds the tracing subscriber within the context of the new tokio runtime.
    /// * Sets the subscriber as configured (globally or thread-local).
    /// * Returns `Self` with the subscriber guard and runtime.
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

    /// Shuts down the background export process. Importantly, this shuts down the guard.
    /// Additionally, it _returns_ the tokio runtime. This is important because the runtime
    /// may _not_ be dropped from the context of another tokio runtime.
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
