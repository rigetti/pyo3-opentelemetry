#[cfg(feature = "export-file")]
pub(crate) mod file;
#[cfg(feature = "export-otlp")]
pub(crate) mod otlp;
#[cfg(feature = "export-py-otlp")]
pub(crate) mod py_otlp;

use std::fmt::Debug;

use opentelemetry_sdk::trace::TracerProvider;
use pyo3::prelude::*;
use tracing_subscriber::{Layer, Registry};

pub(super) type Shutdown = Box<
    dyn (FnOnce() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = ShutdownResult<()>> + Send + Sync>,
        >) + Send
        + Sync,
>;

pub(crate) struct WithShutdown {
    pub(crate) layer: Box<dyn Layer<Registry> + Send + Sync>,
    pub(crate) shutdown: Shutdown,
}

impl core::fmt::Debug for WithShutdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LayerWithShutdown {{ layer: Box<dyn Layer<Registry> + Send + Sync>, shutdown: Shutdown }}")
    }
}

#[derive(thiserror::Error, Debug)]
#[error("{message}")]
pub(crate) struct CustomError {
    message: String,
    #[source]
    source: Box<dyn std::error::Error + Send + Sync>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum BuildError {
    #[cfg(feature = "export-file")]
    #[error("file layer: {0}")]
    File(#[from] file::BuildError),
    #[cfg(feature = "export-otlp")]
    #[error("otlp layer: {0}")]
    Otlp(#[from] otlp::BuildError),
    #[cfg(feature = "export-py-otlp")]
    #[error("custom layer: {0}")]
    Custom(#[from] CustomError),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ShutdownError {
    #[error("custom layer: {0}")]
    Custom(#[from] CustomError),
}

pub(crate) type ShutdownResult<T> = Result<T, ShutdownError>;

pub(super) type LayerBuildResult<T> = Result<T, BuildError>;

pub(crate) trait Config: Send + Sync + BoxDynConfigClone + Debug {
    fn build(&self, batch: bool) -> LayerBuildResult<WithShutdown>;
}

pub(crate) trait BoxDynConfigClone {
    fn clone_box(&self) -> Box<dyn Config>;
}

impl<T> BoxDynConfigClone for T
where
    T: 'static + Config + Clone,
{
    fn clone_box(&self) -> Box<dyn Config> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Config> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

pub(super) fn force_flush_provider_as_shutdown(provider: TracerProvider) -> Shutdown {
    Box::new(
        move || -> std::pin::Pin<Box<dyn std::future::Future<Output = ShutdownResult<()>> + Send + Sync>> {
            Box::pin(async move {
                provider.force_flush();
                Ok(())
            })
        },
    )
}

#[derive(FromPyObject, Clone, Debug)]
pub(crate) enum OtelExportLayerConfig {
    #[cfg(feature = "export-file")]
    File(file::Config),
    #[cfg(feature = "export-otlp")]
    Otlp(otlp::PyConfig),
    #[cfg(feature = "export-py-otlp")]
    PyOtlp(py_otlp::Config),
}

impl Config for OtelExportLayerConfig {
    fn build(&self, batch: bool) -> LayerBuildResult<WithShutdown> {
        match self {
            #[cfg(feature = "export-file")]
            Self::File(config) => config.build(batch),
            #[cfg(feature = "export-otlp")]
            Self::Otlp(config) => otlp::Config::try_from(config.clone())
                .map_err(BuildError::from)?
                .build(batch),
            #[cfg(feature = "export-py-otlp")]
            Self::PyOtlp(config) => config.build(batch),
        }
    }
}
