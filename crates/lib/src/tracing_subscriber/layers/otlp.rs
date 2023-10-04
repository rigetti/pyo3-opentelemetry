use std::{collections::HashMap, path::Path, time::Duration};

use opentelemetry_api::{trace::TraceError, KeyValue};
use opentelemetry_otlp::{TonicExporterBuilder, WithExportConfig};
use opentelemetry_sdk::{
    trace::{Sampler, SpanLimits},
    Resource,
};
use pyo3::prelude::*;

use opentelemetry_sdk::trace;
use rigetti_pyo3::create_init_submodule;
use tonic::metadata::{
    errors::{InvalidMetadataKey, InvalidMetadataValue},
    MetadataKey,
};

use super::{force_flush_provider_as_shutdown, LayerBuildResult, WithShutdown};

impl Config {
    fn initialize_otlp_exporter(&self) -> TonicExporterBuilder {
        let mut otlp_exporter = opentelemetry_otlp::new_exporter().tonic().with_env();
        if let Some(endpoint) = self.endpoint.clone() {
            otlp_exporter = otlp_exporter.with_endpoint(endpoint);
        }
        if let Some(timeout) = self.timeout {
            otlp_exporter = otlp_exporter.with_timeout(timeout);
        }
        if let Some(metadata_map) = self.metadata_map.clone() {
            otlp_exporter = otlp_exporter.with_metadata(metadata_map);
        }
        otlp_exporter
    }
}

impl crate::tracing_subscriber::layers::Config for Config {
    fn build(&self, batch: bool) -> LayerBuildResult<WithShutdown> {
        let pipeline = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(self.initialize_otlp_exporter())
            .with_trace_config(
                trace::config()
                    .with_sampler(self.sampler.clone())
                    .with_span_limits(self.span_limits)
                    .with_resource(self.resource.clone()),
            );

        let tracer = if batch {
            pipeline.install_batch(opentelemetry::runtime::TokioCurrentThread)
        } else {
            pipeline.install_simple()
        }
        .map_err(BuildError::from)?;
        let provider = tracer
            .provider()
            .ok_or(BuildError::ProviderNotSetOnTracer)?;
        let layer = tracing_opentelemetry::layer().with_tracer(tracer);
        Ok(WithShutdown {
            layer: Box::new(layer),
            shutdown: force_flush_provider_as_shutdown(provider),
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    #[error("error in the configuration: {0}")]
    Config(#[from] ConfigError),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum BuildError {
    #[error("failed to build opentelemetry-otlp pipeline: {0}")]
    BatchInstall(#[from] TraceError),
    #[error("provider not set on returned opentelemetry-otlp tracer")]
    ProviderNotSetOnTracer,
    #[error("error in the configuration: {0}")]
    Config(#[from] ConfigError),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ConfigError {
    #[error("invalid metadata map value: {0}")]
    InvalidMetadataValue(#[from] InvalidMetadataValue),
    #[error("invalid metadata map key: {0}")]
    InvalidMetadataKey(#[from] InvalidMetadataKey),
}

#[derive(Clone, Debug)]
pub(crate) struct Config {
    span_limits: SpanLimits,
    resource: Resource,
    metadata_map: Option<tonic::metadata::MetadataMap>,
    sampler: Sampler,
    endpoint: Option<String>,
    timeout: Option<Duration>,
}

#[pyclass(name = "SpanLimits")]
#[derive(Clone, Debug)]
struct PySpanLimits {
    /// The max events that can be added to a `Span`.
    max_events_per_span: u32,
    /// The max attributes that can be added to a `Span`.
    max_attributes_per_span: u32,
    /// The max links that can be added to a `Span`.
    max_links_per_span: u32,
    /// The max attributes that can be added into an `Event`
    max_attributes_per_event: u32,
    /// The max attributes that can be added into a `Link`
    max_attributes_per_link: u32,
}

impl Default for PySpanLimits {
    fn default() -> Self {
        Self::from(SpanLimits::default())
    }
}

impl From<SpanLimits> for PySpanLimits {
    fn from(span_limits: SpanLimits) -> Self {
        Self {
            max_events_per_span: span_limits.max_events_per_span,
            max_attributes_per_span: span_limits.max_attributes_per_span,
            max_links_per_span: span_limits.max_links_per_span,
            max_attributes_per_event: span_limits.max_attributes_per_event,
            max_attributes_per_link: span_limits.max_attributes_per_link,
        }
    }
}

impl From<PySpanLimits> for SpanLimits {
    fn from(span_limits: PySpanLimits) -> Self {
        Self {
            max_events_per_span: span_limits.max_events_per_span,
            max_attributes_per_span: span_limits.max_attributes_per_span,
            max_links_per_span: span_limits.max_links_per_span,
            max_attributes_per_event: span_limits.max_attributes_per_event,
            max_attributes_per_link: span_limits.max_attributes_per_link,
        }
    }
}

#[pymethods]
impl PySpanLimits {
    #[new]
    #[pyo3(signature = (
        /,
        max_events_per_span = None,
        max_attributes_per_span = None,
        max_links_per_span = None,
        max_attributes_per_event = None,
        max_attributes_per_link = None
    ))]
    fn new(
        max_events_per_span: Option<u32>,
        max_attributes_per_span: Option<u32>,
        max_links_per_span: Option<u32>,
        max_attributes_per_event: Option<u32>,
        max_attributes_per_link: Option<u32>,
    ) -> Self {
        let span_limits = Self::default();
        Self {
            max_events_per_span: max_events_per_span.unwrap_or(span_limits.max_events_per_span),
            max_attributes_per_span: max_attributes_per_span
                .unwrap_or(span_limits.max_attributes_per_span),
            max_links_per_span: max_links_per_span.unwrap_or(span_limits.max_links_per_span),
            max_attributes_per_event: max_attributes_per_event
                .unwrap_or(span_limits.max_attributes_per_event),
            max_attributes_per_link: max_attributes_per_link
                .unwrap_or(span_limits.max_attributes_per_link),
        }
    }
}

#[pyclass(name = "Config")]
#[derive(Clone, Default, Debug)]
pub(crate) struct PyConfig {
    span_limits: PySpanLimits,
    resource: PyResource,
    metadata_map: Option<HashMap<String, String>>,
    sampler: PySampler,
    endpoint: Option<String>,
    timeout_millis: Option<u64>,
}

#[pymethods]
impl PyConfig {
    #[new]
    #[pyo3(signature = (
        /,
        span_limits = None,
        resource = None,
        metadata_map = None,
        sampler = None,
        endpoint = None,
        timeout_millis = None 
    ))]
    fn new(
        span_limits: Option<PySpanLimits>,
        resource: Option<PyResource>,
        metadata_map: Option<&PyAny>,
        sampler: Option<&PyAny>,
        endpoint: Option<&str>,
        timeout_millis: Option<u64>,
    ) -> PyResult<Self> {
        Ok(Self {
            span_limits: span_limits.unwrap_or_default(),
            resource: resource.unwrap_or_default(),
            metadata_map: metadata_map.map(PyAny::extract).transpose()?,
            sampler: sampler.map(PyAny::extract).transpose()?.unwrap_or_default(),
            endpoint: endpoint.map(String::from),
            timeout_millis,
        })
    }
}

#[pyclass(name = "Resource")]
#[derive(Clone, Default, Debug)]
struct PyResource {
    attrs: HashMap<String, PyResourceValue>,
    schema_url: Option<String>,
}

#[pymethods]
impl PyResource {
    #[new]
    #[pyo3(signature = (/, attrs = None, schema_url = None))]
    fn new(attrs: Option<HashMap<String, PyResourceValue>>, schema_url: Option<&str>) -> Self {
        Self {
            attrs: attrs.unwrap_or_default(),
            schema_url: schema_url.map(String::from),
        }
    }
}

impl From<PyResource> for Resource {
    fn from(resource: PyResource) -> Self {
        let kvs = resource
            .attrs
            .into_iter()
            .map(|(k, v)| KeyValue::new(k, v))
            .collect::<Vec<KeyValue>>();
        match resource.schema_url {
            Some(schema_url) => Self::from_schema_url(kvs, schema_url),
            None => Self::new(kvs),
        }
    }
}

#[derive(FromPyObject, Clone, Debug, PartialEq)]
pub(crate) enum PyResourceValue {
    /// bool values
    Bool(bool),
    /// i64 values
    I64(i64),
    /// f64 values
    F64(f64),
    /// String values
    String(String),
    /// Array of homogeneous values
    Array(PyResourceValueArray),
}

#[derive(FromPyObject, Debug, Clone, PartialEq)]
pub(crate) enum PyResourceValueArray {
    /// Array of bools
    Bool(Vec<bool>),
    /// Array of integers
    I64(Vec<i64>),
    /// Array of floats
    F64(Vec<f64>),
    /// Array of strings
    String(Vec<String>),
}

impl From<PyResourceValueArray> for opentelemetry_api::Array {
    fn from(py_resource_value_array: PyResourceValueArray) -> Self {
        match py_resource_value_array {
            PyResourceValueArray::Bool(b) => Self::Bool(b),
            PyResourceValueArray::I64(i) => Self::I64(i),
            PyResourceValueArray::F64(f) => Self::F64(f),
            PyResourceValueArray::String(s) => {
                Self::String(s.iter().map(|v| v.clone().into()).collect())
            }
        }
    }
}

impl From<PyResourceValue> for opentelemetry_api::Value {
    fn from(py_resource_value: PyResourceValue) -> Self {
        match py_resource_value {
            PyResourceValue::Bool(b) => Self::Bool(b),
            PyResourceValue::I64(i) => Self::I64(i),
            PyResourceValue::F64(f) => Self::F64(f),
            PyResourceValue::String(s) => Self::String(s.into()),
            PyResourceValue::Array(a) => Self::Array(a.into()),
        }
    }
}

#[allow(variant_size_differences)]
#[derive(FromPyObject, Debug, Clone, PartialEq)]
enum PySampler {
    AlwaysOn(bool),
    TraceIdParentRatioBased(f64),
}

impl Default for PySampler {
    fn default() -> Self {
        Self::AlwaysOn(true)
    }
}

impl From<PySampler> for Sampler {
    fn from(sampler: PySampler) -> Self {
        match sampler {
            PySampler::AlwaysOn(b) if b => Self::AlwaysOn,
            PySampler::AlwaysOn(_) => Self::AlwaysOff,
            PySampler::TraceIdParentRatioBased(f) => Self::TraceIdRatioBased(f),
        }
    }
}

impl TryFrom<PyConfig> for Config {
    type Error = BuildError;

    fn try_from(config: PyConfig) -> Result<Self, Self::Error> {
        let metadata_map = match config.metadata_map {
            Some(m) => Some(m.into_iter().try_fold(
                tonic::metadata::MetadataMap::new(),
                |mut metadata_map: tonic::metadata::MetadataMap,
                 (k, v)|
                 -> Result<_, Self::Error> {
                    let key = k.parse::<MetadataKey<_>>().map_err(ConfigError::from)?;
                    metadata_map.insert(key, v.parse().map_err(ConfigError::from)?);
                    Ok(metadata_map)
                },
            )?),
            None => None,
        };

        Ok(Self {
            span_limits: config.span_limits.into(),
            resource: config.resource.into(),
            metadata_map,
            sampler: config.sampler.into(),
            endpoint: config.endpoint,
            timeout: config.timeout_millis.map(Duration::from_millis),
        })
    }
}

create_init_submodule! {
    classes: [ PyConfig, PySpanLimits, PyResource ],
}

#[allow(dead_code)]
pub(super) fn build_stub_files(directory: &Path) -> Result<(), std::io::Error> {
    let data = include_bytes!("../../../assets/python_stubs/layers/otlp/__init__.pyi");
    std::fs::create_dir_all(directory)?;
    let init_file = directory.join("__init__.pyi");
    std::fs::write(init_file, data)
}
