use std::{collections::HashMap, time::Duration};

use opentelemetry_api::{trace::TraceError, KeyValue};
use opentelemetry_otlp::{TonicExporterBuilder, WithExportConfig};
use opentelemetry_sdk::{
    trace::{Sampler, SpanLimits},
    Resource,
};
use pyo3::{exceptions::PyValueError, prelude::*};

use opentelemetry_sdk::trace;
use rigetti_pyo3::{create_init_submodule, ToPythonError};
use tonic::metadata::{
    errors::{InvalidMetadataKey, InvalidMetadataValue},
    MetadataKey,
};

#[derive(thiserror::Error, Debug)]
enum ConfigError {
    #[error("invalid metadata map value: {0}")]
    InvalidMetadataValue(#[from] InvalidMetadataValue),
    #[error("invalid metadata map key: {0}")]
    InvalidMetadataKey(#[from] InvalidMetadataKey),
}

impl ToPythonError for ConfigError {
    fn to_py_err(self) -> PyErr {
        match self {
            Self::InvalidMetadataValue(e) => PyValueError::new_err(e.to_string()),
            Self::InvalidMetadataKey(e) => PyValueError::new_err(e.to_string()),
        }
    }
}

#[derive(Clone)]
struct Config {
    span_limits: SpanLimits,
    resource: Resource,
    metadata_map: Option<tonic::metadata::MetadataMap>,
    sampler: Sampler,
    endpoint: Option<String>,
    timeout: Option<Duration>,
}

impl Config {
    fn build_oltp_exporter(&self) -> TonicExporterBuilder {
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

#[pyclass]
#[derive(Clone)]
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

#[pyclass]
#[derive(Clone)]
struct PyConfig {
    span_limits: PySpanLimits,
    resource: PyResource,
    metadata_map: Option<HashMap<String, String>>,
    sampler: PySampler,
    endpoint: Option<String>,
    timeout_millis: Option<u64>,
}

#[derive(Clone)]
struct PyResource {
    attrs: HashMap<String, PyResourceValue>,
    schema_url: Option<String>,
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
pub enum PyResourceValue {
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
pub enum PyResourceValueArray {
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
    type Error = ConfigError;

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

#[derive(thiserror::Error, Debug)]
pub(crate) enum TracerInitializationError {
    #[error("provider not set on returned opentelemetry-otlp tracer")]
    ProviderNotSetOnTracer,
    #[error("failed to install batch exporter on opentelemetry-otlp pipeline")]
    BatchInstall(#[from] TraceError),
}

#[pyclass]
pub(super) struct OTLPExporter {
    config: Config,
}

#[pymethods]
impl OTLPExporter {
    #[new]
    fn new(config: PyConfig) -> PyResult<Self> {
        Ok(Self {
            config: config.try_into().map_err(ToPythonError::to_py_err)?,
        })
    }

    fn __aenter__(&self) -> PyResult<()> {
        let config = self.config.clone();
        super::util::start_tracer(
            move || -> Result<_, super::util::trace::TracerInitializationError> {
                let otlp_exporter = config.build_oltp_exporter();
                // Then pass it into pipeline builder
                let tracer = opentelemetry_otlp::new_pipeline()
                    .tracing()
                    .with_exporter(otlp_exporter)
                    .with_trace_config(
                        trace::config()
                            .with_sampler(config.sampler)
                            .with_span_limits(config.span_limits)
                            .with_resource(config.resource),
                    )
                    .install_batch(opentelemetry::runtime::TokioCurrentThread)
                    .map_err(TracerInitializationError::from)?;
                let provider = tracer
                    .provider()
                    .ok_or(TracerInitializationError::ProviderNotSetOnTracer)?;
                Ok((provider, tracer))
            },
        )
        .map_err(ToPythonError::to_py_err)
    }

    #[staticmethod]
    fn __aexit__<'a>(
        py: Python<'a>,
        _exc_type: Option<&PyAny>,
        _exc_value: Option<&PyAny>,
        _traceback: Option<&PyAny>,
    ) -> PyResult<&'a PyAny> {
        super::util::stop(py)
    }
}

create_init_submodule! {
    classes: [ OTLPExporter ],
}
