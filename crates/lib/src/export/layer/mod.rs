pub(crate) mod file;
pub(crate) mod otlp;
pub(crate) mod py_otlp;

use opentelemetry_sdk::trace::TracerProvider;
use pyo3::prelude::*;
use tracing_subscriber::{Layer, Registry};

pub(super) type Shutdown = Box<
    dyn (FnOnce() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = LayerResult<()>> + Send + Sync>,
        >) + Send
        + Sync,
>;

pub struct LayerWithShutdown {
    pub layer: Box<dyn Layer<Registry> + Send + Sync>,
    pub shutdown: Shutdown,
}

#[derive(thiserror::Error, Debug)]
#[error("{message}")]
pub struct CustomError {
    message: String,
    #[source]
    source: Box<dyn std::error::Error + Send + Sync>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum BuildError {
    #[error("file layer: {0}")]
    File(#[from] file::BuildError),
    #[error("otlp layer: {0}")]
    Otlp(#[from] otlp::BuildError),
    #[error("custom layer: {0}")]
    Custom(#[from] CustomError),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ShutdownError {
    #[error("custom layer: {0}")]
    Custom(#[from] CustomError),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum LayerError {
    #[error("otlp layer: {0}")]
    Otlp(#[from] otlp::Error),
    #[error("py otlp layer: {0}")]
    PyOtlp(#[from] py_otlp::Error),
    #[error("file layer: {0}")]
    File(#[from] file::BuildError),
    #[error(transparent)]
    Custom(#[from] CustomError),
}

pub type LayerResult<T> = Result<T, LayerError>;

pub(super) type LayerBuildResult<T> = Result<T, BuildError>;

pub trait Config: Send + Sync + BoxDynConfigClone {
    fn build(&self, batch: bool) -> LayerBuildResult<LayerWithShutdown>;
}

trait BoxDynConfigClone {
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
    fn clone(&self) -> Box<dyn Config> {
        self.clone_box()
    }
}

pub(super) fn force_flush_provider_as_shutdown(provider: TracerProvider) -> Shutdown {
    Box::new(
        move || -> std::pin::Pin<Box<dyn std::future::Future<Output = LayerResult<()>> + Send + Sync>> {
            Box::pin(async move {
                provider.force_flush();
                Ok(())
            })
        },
    )
}

#[pyclass]
#[pyo3(name = "Config")]
pub struct PyConfig {
    layer_config: Box<dyn Config>,
}

#[derive(FromPyObject, Clone)]
pub(crate) enum OtelExportLayerConfig {
    File(file::Config),
    Otlp(otlp::PyConfig),
    PyOtlp(py_otlp::Config),
}

impl Config for OtelExportLayerConfig {
    fn build(&self, batch: bool) -> LayerBuildResult<LayerWithShutdown> {
        match self {
            Self::File(config) => config.build(batch),
            Self::Otlp(config) => otlp::Config::try_from(config.clone())
                .map_err(BuildError::from)?
                .build(batch),
            Self::PyOtlp(config) => config.build(batch),
        }
    }
}
