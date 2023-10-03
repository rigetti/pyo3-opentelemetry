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

const DEFAULT_TIMEOUT_MILLIS: u64 = 3000;

#[pyclass]
#[derive(Clone, Debug)]
pub(crate) struct BatchConfig {
    pub(super) subscriber: PyConfig,
    pub(super) timeout_millis: u64,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            subscriber: PyConfig::default(),
            timeout_millis: DEFAULT_TIMEOUT_MILLIS,
        }
    }
}

#[pymethods]
impl BatchConfig {
    #[new]
    #[pyo3(signature = (subscriber = None, timeout_millis = DEFAULT_TIMEOUT_MILLIS))]
    #[allow(clippy::pedantic)]
    fn new(subscriber: Option<PyConfig>, timeout_millis: u64) -> PyResult<Self> {
        #[cfg(any(feature = "export-file", feature = "export-otlp"))]
        let subscriber = subscriber.unwrap_or_default();
        #[cfg(all(not(feature = "export-file"), not(feature = "export-otlp")))]
        let subscriber = crate::tracing_subscriber::unsupported_default_initialization(subscriber)?;
        Ok(Self {
            subscriber,
            timeout_millis,
        })
    }
}

#[pyclass]
#[derive(Clone, Debug, Default)]
pub(crate) struct SimpleConfig {
    pub(super) subscriber: PyConfig,
}

#[pymethods]
impl SimpleConfig {
    #[new]
    #[pyo3(signature = (subscriber = None))]
    #[allow(clippy::pedantic)]
    fn new(subscriber: Option<PyConfig>) -> PyResult<Self> {
        #[cfg(any(feature = "export-file", feature = "export-otlp"))]
        let subscriber = subscriber.unwrap_or_default();
        #[cfg(all(not(feature = "export-file"), not(feature = "export-otlp")))]
        let subscriber = crate::tracing_subscriber::unsupported_default_initialization(subscriber)?;
        Ok(Self { subscriber })
    }
}

#[derive(FromPyObject, Clone, Debug)]
pub(crate) enum ExportProcessConfig {
    Batch(BatchConfig),
    Simple(SimpleConfig),
}

#[cfg(any(feature = "export-file", feature = "export-otlp"))]
impl Default for ExportProcessConfig {
    fn default() -> Self {
        Self::Batch(BatchConfig::default())
    }
}

pub(crate) enum ExportProcess {
    GlobalBatch(global_batch::ExportProcess),
    GlobalSimple(global_simple::ExportProcess),
    CurrentThreadSimple(current_thread_simple::ExportProcess),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum InitializationError {
    #[error("global batch export: {0}")]
    GlobalBatch(#[from] global_batch::InitializationError),
    #[error("failed to build subscriber: {0}")]
    SubscriberBuild(#[from] crate::tracing_subscriber::subscriber::BuildError),
}

wrap_error!(RustTracingInitializationError(InitializationError));
py_wrap_error!(
    export_process,
    RustTracingInitializationError,
    TracingInitializationError,
    PyRuntimeError
);

#[derive(thiserror::Error, Debug)]
pub(crate) enum StartError {
    #[error("failed to start global batch: {0}")]
    GlobalBatch(#[from] global_batch::StartError),
    #[error("failed to set global default tracing subscriber: {0}")]
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
    #[error("the subscriber failed to shutdown: {0}")]
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
    type Error = InitializationError;

    fn try_from(config: TracingConfig) -> Result<Self, InitializationError> {
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
            TracingConfig::CurrentThread(config) => match config.export_process {
                ExportProcessConfig::Batch(_) => {
                    todo!("current thread batch export is not yet implemented")
                }
                ExportProcessConfig::Simple(config) => {
                    let subscriber = config.subscriber.subscriber_config.build(false)?;
                    let process = current_thread_simple::ExportProcess::new(subscriber);
                    Ok(Self::CurrentThreadSimple(process))
                }
            },
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
    errors: [TracingStartError, TracingShutdownError, TracingInitializationError],
}
