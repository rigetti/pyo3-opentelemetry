//! This module contains a limited set of tracing layers which can be used to configure the
//! [`tracing_subscriber::Registry`] for use with the [`Tracing`] context manager.
//!
//! Currently, the following layers are supported:
//!
//! * [`crate::layers::otel_file::Config`] - a layer which writes spans to a file (or stdout) in the `OpenTelemetry` OTLP
//! JSON-serialized format.
//! * [`crate::layers::otel_otlp::Config`] - a layer which exports spans to an `OpenTelemetry` collector.
#[cfg(feature = "layer-otel-file")]
pub(crate) mod otel_file;
#[cfg(feature = "layer-otel-otlp")]
pub(crate) mod otel_otlp;

use std::{fmt::Debug, time::Duration};

use opentelemetry_sdk::trace::TracerProvider;
use pyo3::prelude::*;
use tracing_subscriber::{Layer, Registry};

pub(super) type Shutdown = Box<
    dyn (FnOnce() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = ShutdownResult<()>> + Send + Sync>,
        >) + Send
        + Sync,
>;

/// Carries the built tracing subscriber layer and a shutdown function that can later be used to
/// shutdown the subscriber upon context manager exit.
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
pub(crate) enum BuildError {
    #[cfg(feature = "layer-otel-file")]
    #[error("file layer: {0}")]
    File(#[from] otel_file::BuildError),
    #[cfg(feature = "layer-otel-otlp")]
    #[error("otlp layer: {0}")]
    Otlp(#[from] otel_otlp::BuildError),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ShutdownError {
    // TODO: This will eventually accept a `CustomError` that can be set by upstream libraries.
}

pub(crate) type ShutdownResult<T> = Result<T, ShutdownError>;

pub(super) type LayerBuildResult<T> = Result<T, BuildError>;

pub(crate) trait Config: Send + Sync + BoxDynConfigClone + Debug {
    fn build(&self, batch: bool) -> LayerBuildResult<WithShutdown>;
    fn requires_runtime(&self) -> bool;
}

pub(crate) trait BoxDynConfigClone {
    fn clone_box(&self) -> Box<dyn Config>;
}

/// This trait is necessary so that `Box<dyn Config>` can be cloned and, therefore,
/// used as an attribute on a `pyo3` class.
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

pub(super) fn force_flush_provider_as_shutdown(
    provider: TracerProvider,
    timeout: Option<Duration>,
) -> Shutdown {
    Box::new(
        move || -> std::pin::Pin<Box<dyn std::future::Future<Output = ShutdownResult<()>> + Send + Sync>> {
            Box::pin(async move {
                if let Some(timeout) = timeout {
                    tokio::time::sleep(timeout).await;
                }
                provider.force_flush();
                Ok(())
            })
        },
    )
}

/// A Python union of one of the supported layers.
#[derive(FromPyObject, Clone, Debug)]
#[allow(variant_size_differences, clippy::large_enum_variant)]
pub(crate) enum PyConfig {
    #[cfg(feature = "layer-otel-file")]
    File(otel_file::Config),
    #[cfg(feature = "layer-otel-otlp")]
    Otlp(otel_otlp::PyConfig),
}

#[cfg(any(feature = "layer-otel-file", feature = "layer-otel-otlp"))]
impl Default for PyConfig {
    fn default() -> Self {
        #[cfg(feature = "layer-otel-file")]
        {
            Self::File(otel_file::Config::default())
        }
        #[cfg(all(feature = "layer-otel-otlp", not(feature = "layer-otel-file")))]
        {
            Self::Otlp(otel_otlp::PyConfig::default())
        }
    }
}

impl Config for PyConfig {
    fn build(&self, batch: bool) -> LayerBuildResult<WithShutdown> {
        match self {
            #[cfg(feature = "layer-otel-file")]
            Self::File(config) => config.build(batch),
            #[cfg(feature = "layer-otel-otlp")]
            Self::Otlp(config) => config.build(batch),
        }
    }

    fn requires_runtime(&self) -> bool {
        match self {
            #[cfg(feature = "layer-otel-file")]
            Self::File(config) => config.requires_runtime(),
            #[cfg(feature = "layer-otel-otlp")]
            Self::Otlp(config) => config.requires_runtime(),
        }
    }
}

/// Adds `layers` submodule to the root level submodule.
#[allow(dead_code)]
pub(crate) fn init_submodule(name: &str, py: Python, m: &PyModule) -> PyResult<()> {
    let modules = py.import("sys")?.getattr("modules")?;

    #[cfg(feature = "layer-otel-file")]
    {
        let submod = pyo3::types::PyModule::new(py, "otel_file")?;
        let qualified_name = format!("{name}.otel_file");
        otel_file::init_submodule(qualified_name.as_str(), py, submod)?;
        modules.set_item(qualified_name, submod)?;
        m.add_submodule(submod)?;
    }
    #[cfg(feature = "layer-otel-otlp")]
    {
        let submod = pyo3::types::PyModule::new(py, "otel_otlp")?;
        let qualified_name = format!("{name}.otel_otlp");
        otel_otlp::init_submodule(qualified_name.as_str(), py, submod)?;
        modules.set_item(qualified_name, submod)?;
        m.add_submodule(submod)?;
    }

    Ok(())
}
