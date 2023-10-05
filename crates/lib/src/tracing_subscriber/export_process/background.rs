use std::sync::Arc;

use tokio::{
    runtime::{Builder, Runtime},
    sync::Notify,
};

use crate::tracing_subscriber::subscriber::{set_subscriber, Config as SubscriberConfig};

use tracing::subscriber::SetGlobalDefaultError;

use super::StartResult;

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
    shutdown_notify: Arc<Notify>,
    pub(super) runtime: Runtime,
}

impl ExportProcess {
    pub(super) fn new(shutdown_notify: Arc<Notify>, runtime: Runtime) -> Self {
        Self {
            shutdown_notify,
            runtime,
        }
    }

    pub(super) fn start(
        subscriber_config: Box<dyn SubscriberConfig>,
        global: bool,
    ) -> StartResult<Self> {
        let runtime = init_runtime()?;
        let shutdown_notify_rx = Arc::new(Notify::new());
        let subscriber = runtime
            .block_on(async move { subscriber_config.build(true).map_err(StartError::from) })?;
        let guard = set_subscriber(subscriber, global)?;
        let shutdown_notify_tx = shutdown_notify_rx.clone();
        // TODO Do we actually need this?
        runtime.spawn(async move {
            shutdown_notify_tx.notified().await;
            guard.shutdown().await.unwrap();
            // notify the shutdown is complete
            shutdown_notify_tx.notify_one();
        });
        Ok(Self::new(shutdown_notify_rx, runtime))
    }

    pub(super) async fn shutdown(self) -> Runtime {
        // notify the background process to shutdown
        self.shutdown_notify.notify_one();
        // wait to be notified that the shutdown is complete
        self.shutdown_notify.notified().await;
        self.runtime
    }
}

fn init_runtime() -> Result<Runtime, StartError> {
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(StartError::RuntimeInitialization)
}
