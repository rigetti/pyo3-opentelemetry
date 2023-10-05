use std::fmt::Debug;

use crate::tracing_subscriber::subscriber::PyConfig;
use pyo3::{exceptions::PyRuntimeError, prelude::*};
use rigetti_pyo3::{create_init_submodule, py_wrap_error, wrap_error};
use tokio::runtime::Runtime;

use super::{
    contextmanager::TracingConfig,
    subscriber::{self, set_subscriber, SetSubscriberError, SubscriberManagerGuard},
};

mod background;

#[pyclass]
#[derive(Clone, Debug, Default)]
pub(crate) struct BatchConfig {
    pub(super) subscriber: PyConfig,
}

#[pymethods]
impl BatchConfig {
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

#[derive(thiserror::Error, Debug)]
pub(crate) enum StartError {
    #[error("failed to start global batch: {0}")]
    GlobalBatch(#[from] background::StartError),
    #[error("failed to build subscriber {0}")]
    BuildSubscriber(#[from] subscriber::BuildError),
    #[error("failed to set subscriber: {0}")]
    SetSubscriber(#[from] SetSubscriberError),
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

pub(crate) enum ExportProcess {
    Background(background::ExportProcess),
    Foreground(SubscriberManagerGuard),
}

impl Debug for ExportProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Background(_) => f
                .debug_struct("ExportProcess::Background")
                .field("process", &"process")
                .finish(),
            Self::Foreground(_) => f
                .debug_struct("ExportProcess::Foreground")
                .field("guard", &"guard")
                .finish(),
        }
    }
}

impl ExportProcess {
    pub(crate) fn start(config: TracingConfig) -> StartResult<Self> {
        match config {
            TracingConfig::Global(config) => match config.export_process {
                ExportProcessConfig::Batch(config) => Ok(Self::Background(
                    background::ExportProcess::start(config.subscriber.subscriber_config, true)?,
                )),
                ExportProcessConfig::Simple(config) => {
                    let requires_runtime = config.subscriber.subscriber_config.requires_runtime();
                    if requires_runtime {
                        Ok(Self::Background(background::ExportProcess::start(
                            config.subscriber.subscriber_config,
                            true,
                        )?))
                    } else {
                        let subscriber = config.subscriber.subscriber_config.build(false)?;
                        Ok(Self::Foreground(set_subscriber(subscriber, true)?))
                    }
                }
            },
            TracingConfig::CurrentThread(config) => match config.export_process {
                ExportProcessConfig::Batch(config) => Ok(Self::Background(
                    background::ExportProcess::start(config.subscriber.subscriber_config, false)?,
                )),
                ExportProcessConfig::Simple(config) => {
                    let requires_runtime = config.subscriber.subscriber_config.requires_runtime();
                    if requires_runtime {
                        Ok(Self::Background(background::ExportProcess::start(
                            config.subscriber.subscriber_config,
                            false,
                        )?))
                    } else {
                        let subscriber = config.subscriber.subscriber_config.build(false)?;
                        Ok(Self::Foreground(set_subscriber(subscriber, false)?))
                    }
                }
            },
        }
    }

    pub(crate) async fn shutdown(self) -> ShutdownResult<Option<Runtime>> {
        match self {
            Self::Background(process) => Ok(Some(process.shutdown().await)),
            Self::Foreground(guard) => {
                guard.shutdown().await?;
                Ok(None)
            }
        }
    }
}

create_init_submodule! {
    errors: [TracingStartError, TracingShutdownError],
}
