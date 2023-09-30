use std::time::Duration;

use crate::tracing_subscriber::subscriber::PyConfig;
use pyo3::{exceptions::PyRuntimeError, prelude::*};
use rigetti_pyo3::{create_init_submodule, py_wrap_error, wrap_error};
use tokio::runtime::Runtime;
use tracing::subscriber::SetGlobalDefaultError;

use super::contextmanager::TracingConfig;

mod current_thread_simple;
mod global_batch;
mod global_simple;

#[pyclass]
#[derive(Clone, Debug)]
pub(crate) struct BatchConfig {
    pub(super) subscriber: PyConfig,
    pub(super) timeout_millis: u64,
}

#[pyclass]
#[derive(Clone, Debug)]
pub(crate) struct SimpleConfig {
    pub(super) subscriber: PyConfig,
}

#[derive(FromPyObject, Clone, Debug)]
pub(crate) enum ExportProcessConfig {
    Batch(BatchConfig),
    Simple(SimpleConfig),
}

pub(crate) enum ExportProcess {
    GlobalBatch(global_batch::ExportProcess),
    GlobalSimple(global_simple::ExportProcess),
    CurrentThreadSimple(current_thread_simple::ExportProcess),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ConfigError {
    #[error("global batch export: {0}")]
    GlobalBatchInitialization(#[from] global_batch::InitializationError),
    #[error("failed to build subscriber: {0}")]
    SubscriberBuild(#[from] crate::tracing_subscriber::subscriber::BuildError),
}

wrap_error!(RustTracingConfigError(ConfigError));
py_wrap_error!(
    export_process,
    RustTracingConfigError,
    TracingConfigurationError,
    PyRuntimeError
);

#[derive(thiserror::Error, Debug)]
pub(crate) enum StartError {
    #[error("failed to start global batch")]
    GlobalBatch(#[from] global_batch::StartError),
    #[error("failed to set global default tracing subscriber")]
    SetSubscriber(#[from] SetGlobalDefaultError),
}

wrap_error!(RustTracingStartError(StartError));
py_wrap_error!(
    export_process,
    RustTracingStartError,
    TracingStartError,
    PyRuntimeError
);

type StartResult<T> = Result<T, StartError>;

#[derive(thiserror::Error, Debug)]
pub(crate) enum ShutdownError {
    #[error("the subscriber failed to shutdown")]
    Subscriber(#[from] crate::tracing_subscriber::subscriber::ShutdownError),
}

wrap_error!(RustTracingShutdownError(ShutdownError));
py_wrap_error!(
    export_process,
    RustTracingShutdownError,
    TracingShutdownError,
    PyRuntimeError
);

type ShutdownResult<T> = Result<T, ShutdownError>;

impl TryFrom<TracingConfig> for ExportProcess {
    type Error = ConfigError;

    fn try_from(config: TracingConfig) -> Result<Self, ConfigError> {
        match config {
            TracingConfig::Global(config) => match config.export_process {
                ExportProcessConfig::Batch(config) => {
                    Ok(Self::GlobalBatch(global_batch::ExportProcess::new(
                        config.subscriber.subscriber_config,
                        Duration::from_millis(config.timeout_millis),
                    )?))
                }
                ExportProcessConfig::Simple(config) => {
                    let subscriber = config.subscriber.subscriber_config.build(false)?;
                    let process = global_simple::ExportProcess::new(subscriber);
                    Ok(Self::GlobalSimple(process))
                }
            },
            TracingConfig::CurrentThread(config) => {
                let subscriber = config.subscriber.subscriber_config.build(false)?;
                let process = current_thread_simple::ExportProcess::new(subscriber);
                Ok(Self::CurrentThreadSimple(process))
            }
        }
    }
}

impl ExportProcess {
    pub(crate) fn start(&mut self) -> StartResult<()> {
        match self {
            Self::GlobalBatch(process) => Ok(process.start_tracer()?),
            Self::GlobalSimple(process) => process.start_tracer(),
            Self::CurrentThreadSimple(process) => {
                process.start_tracer();
                Ok(())
            }
        }
    }

    pub(crate) async fn shutdown(self) -> ShutdownResult<Option<Runtime>> {
        match self {
            Self::GlobalBatch(process) => Ok(Some(process.shutdown().await)),
            Self::GlobalSimple(process) => {
                process.shutdown().await?;
                Ok(None)
            }
            Self::CurrentThreadSimple(process) => {
                process.shutdown().await?;
                Ok(None)
            }
        }
    }
}

create_init_submodule! {
    errors: [TracingStartError, TracingShutdownError, TracingConfigurationError],
}
