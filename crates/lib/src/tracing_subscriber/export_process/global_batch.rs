use std::{sync::Arc, time::Duration};

use tokio::{
    runtime::{Builder, Runtime},
    sync::Notify,
};

use crate::tracing_subscriber::subscriber::Config as SubscriberConfig;

use tracing::subscriber::SetGlobalDefaultError;

#[derive(thiserror::Error, Debug)]
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
    subscriber_config: Box<dyn SubscriberConfig>,
    timeout: Duration,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum InitializationError {
    #[error("failed to initialize export background tokio runtime")]
    RuntimeInitialization(#[from] std::io::Error),
}

type InitializationResult<T> = Result<T, InitializationError>;

impl ExportProcess {
    pub(super) fn new(
        subscriber_config: Box<dyn SubscriberConfig>,
        timeout: Duration,
    ) -> InitializationResult<Self> {
        let runtime = init_runtime()?;
        let shutdown_notify = Arc::new(Notify::new());
        Ok(Self {
            shutdown_notify,
            runtime,
            subscriber_config,
            timeout,
        })
    }

    pub(super) fn start_tracer(&self) -> Result<(), StartError> {
        let (set_subscriber_result_tx, set_subscriber_result_rx) = tokio::sync::oneshot::channel();
        let shutdown_notify = self.shutdown_notify.clone();
        let subscriber_config = self.subscriber_config.clone();
        self.runtime.spawn(async move {
            let subscriber = subscriber_config
                .build(true)
                .map_err(StartError::from)
                .and_then(|mut subscriber_with_shutdown| {
                    // TODO we'll need to inject this for testing
                    let subscriber = subscriber_with_shutdown.subscriber.take().unwrap();
                    tracing::subscriber::set_global_default(subscriber)
                        .map(|_| subscriber_with_shutdown)
                        .map_err(StartError::from)
                });
            let (subscriber, initialization_result) = match subscriber {
                Ok(subscriber) => (Some(subscriber), Ok(())),
                Err(e) => (None, Err(e)),
            };
            if let Err(initialization_result) = set_subscriber_result_tx.send(initialization_result)
            {
                // In this case, the receiver never receives the "ready" signal, so
                // the `start_tracer` function will timeout and return an errors, so we do
                // not need to wait for a shutdown and force flush.
                if let Err(e) = initialization_result {
                    eprintln!("failed to send unsuccessful subscriber initialization signal: {e}",);
                } else {
                    eprintln!("failed to send successful subscriber initialization signal");
                }
                return;
            }
            if let Some(subscriber) = subscriber {
                // wait for shutdown notification
                shutdown_notify.notified().await;
                let shutdown = subscriber.shutdown;
                shutdown().await.unwrap();
                // notify the shutdown is complete
                shutdown_notify.notify_one();
            }
        });

        // let handle = Handle::try_current().unwrap();
        // let _guard = handle.enter();
        // We should not be in an existing tokio runtime, so we create a new one
        // and block on the result of the `set_subscriber` function. This ensures
        // the function does not return until the subscriber is set and we are ready
        // to start collecting trace data.
        let wait_for_startup_runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(StartError::RuntimeInitialization)?;
        let _guard = wait_for_startup_runtime.enter();
        wait_for_startup_runtime
            .handle()
            .block_on(tokio::time::timeout(self.timeout, set_subscriber_result_rx))
            .map_err(StartError::from)
            .and_then(|r| {
                r.map_err(StartError::from)
                    .and_then(|r| r.map_err(StartError::from))
            })
    }

    pub(super) async fn shutdown(self) -> Runtime {
        // notify the background process to shutdown
        self.shutdown_notify.notify_one();
        // wait to be notified that the shutdown is complete
        self.shutdown_notify.notified().await;
        self.runtime
    }
}

fn init_runtime() -> Result<Runtime, InitializationError> {
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(InitializationError::RuntimeInitialization)
}
