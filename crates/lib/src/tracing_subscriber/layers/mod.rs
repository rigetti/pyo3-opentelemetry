#[cfg(feature = "export-file")]
pub(crate) mod file;
#[cfg(feature = "export-otlp")]
pub(crate) mod otlp;
#[cfg(feature = "export-py-otlp")]
pub(crate) mod py_otlp;

use std::fmt::Debug;

use opentelemetry_sdk::trace::TracerProvider;
use pyo3::prelude::*;
use rigetti_pyo3::create_init_submodule;
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
enum OtelExportLayerConfig {
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

#[pyclass(name = "Config")]
#[derive(Clone)]
pub(crate) struct PyConfig {
    pub(crate) layer_config: Box<dyn Config>,
}

impl core::fmt::Debug for PyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PyConfig {{ layer_config: Box<dyn Config> }}")
    }
}

#[pymethods]
impl PyConfig {
    #[new]
    pub(crate) fn new(layer_config: &PyAny) -> PyResult<Self> {
        let layer_config = layer_config.extract::<OtelExportLayerConfig>()?;
        Ok(Self {
            layer_config: Box::new(layer_config),
        })
    }
}

/// Adds the pyo3-opentelemetry export module to your parent module. The upshot here
/// is that the Python package will contain `{name}.export.{stdout/otlp/py_otlp}`,
/// each with an async context manager that can be used on the Python side to
/// export spans.
///
/// # Arguments
/// * `name` - The name of the parent module.
/// * `py` - The Python interpreter.
/// * `m` - The parent module.
///
/// # Returns
/// * `PyResult<()>` - The result of adding the submodule to the parent module.
///
/// # Errors
/// * If the submodule cannot be added to the parent module.
///
pub(crate) fn init_submodule(name: &str, py: Python, m: &PyModule) -> PyResult<()> {
    let modules = py.import("sys")?.getattr("modules")?;

    #[cfg(feature = "export-file")]
    {
        let submod = pyo3::types::PyModule::new(py, "file")?;
        file::init_submodule("file", py, submod)?;
        modules.set_item(format!("{name}.open_telemetry.file"), submod)?;
        m.add_submodule(submod)?;
    }
    #[cfg(feature = "export-otlp")]
    {
        let submod = pyo3::types::PyModule::new(py, "otlp")?;
        otlp::init_submodule("otlp", py, submod)?;
        modules.set_item(format!("{name}.open_telemetry.otlp"), submod)?;
        m.add_submodule(submod)?;
    }
    #[cfg(feature = "export-py-otlp")]
    {
        let submod = pyo3::types::PyModule::new(py, "py_otlp")?;
        py_otlp::init_submodule("py_otlp", py, submod)?;
        modules.set_item(format!("{name}.open_telemetry.py_otlp"), submod)?;
        m.add_submodule(submod)?;
    }

    m.add_class::<PyConfig>()?;

    Ok(())
}
