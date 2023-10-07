use std::{collections::HashMap, env, time::Duration};

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
use tracing_subscriber::{
    filter::{FromEnvError, ParseError},
    EnvFilter, Layer,
};

use super::{force_flush_provider_as_shutdown, LayerBuildResult, WithShutdown};

/// Configures the [`opentelemetry-otlp`] crate layer.
#[derive(Clone, Debug)]
pub(crate) struct Config {
    /// Configuration to limit the amount of trace data collected.
    span_limits: SpanLimits,
    /// OpenTelemetry resource attributes describing the entity that produced the telemetry.
    resource: Resource,
    /// The metadata map to use for requests to the remote collector.
    metadata_map: Option<tonic::metadata::MetadataMap>,
    /// The sampler to use for the [`opentelemetry::sdk::trace::TracerProvider`].
    sampler: Sampler,
    /// The endpoint to which the exporter will send trace data. If not set, this must be set by
    /// OTLP environment variables.
    endpoint: Option<String>,
    /// Timeout applied the [`tonic::transport::Channel`] used to send trace data to the remote collector.
    timeout: Option<Duration>,
    /// A timeout applied to the shutdown of the [`crate::contextmanager::Tracing`] context
    /// manager upon exiting, before the underlying [`opentelemetry::sdk::trace::TracerProvider`]
    /// is shutdown. Ensures that spans are flushed before the program exits.
    pre_shutdown_timeout: Duration,
    /// The filter to use for the [`EnvFilter`] layer.
    env_filter: Option<String>,
}

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

impl crate::layers::Config for PyConfig {
    fn requires_runtime(&self) -> bool {
        Config::requires_runtime()
    }
    fn build(&self, batch: bool) -> LayerBuildResult<WithShutdown> {
        Config::try_from(self.clone())
            .map_err(BuildError::from)?
            .build(batch)
    }
}

/// An environment variable that can be used to set an [`EnvFilter`] for the OTLP layer.
/// This supersedes the `RUST_LOG` environment variable, but is superseded by the
/// [`Config::env_filter`] field.
const PYO3_OPENTELEMETRY_ENV_FILTER: &str = "PYO3_OPENTELEMETRY_ENV_FILTER";

impl Config {
    const fn requires_runtime() -> bool {
        true
    }

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
        let env_filter = self
            .env_filter
            .clone()
            .or_else(|| env::var(PYO3_OPENTELEMETRY_ENV_FILTER).ok())
            .map_or_else(
                || EnvFilter::try_from_default_env().map_err(BuildError::from),
                |filter| EnvFilter::try_new(filter).map_err(BuildError::from),
            )?;
        let layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(env_filter);
        Ok(WithShutdown {
            layer: Box::new(layer),
            shutdown: force_flush_provider_as_shutdown(provider, Some(self.pre_shutdown_timeout)),
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
    #[error("failed to parse specified trace filter: {0}")]
    TraceFilterParseError(#[from] ParseError),
    #[error("failed to parse trace filter from RUST_LOG: {0}")]
    TraceFilterEnvError(#[from] FromEnvError),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ConfigError {
    #[error("invalid metadata map value: {0}")]
    InvalidMetadataValue(#[from] InvalidMetadataValue),
    #[error("invalid metadata map key: {0}")]
    InvalidMetadataKey(#[from] InvalidMetadataKey),
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

/// A Python representation of [`Config`].
#[pyclass(name = "Config")]
#[derive(Clone, Default, Debug)]
pub(crate) struct PyConfig {
    span_limits: PySpanLimits,
    resource: PyResource,
    metadata_map: Option<HashMap<String, String>>,
    sampler: PySampler,
    endpoint: Option<String>,
    timeout_millis: Option<u64>,
    pre_shutdown_timeout_millis: u64,
    env_filter: Option<String>,
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
        timeout_millis = None,
        pre_shutdown_timeout_millis = 2000,
        env_filter = None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        span_limits: Option<PySpanLimits>,
        resource: Option<PyResource>,
        metadata_map: Option<&PyAny>,
        sampler: Option<&PyAny>,
        endpoint: Option<&str>,
        timeout_millis: Option<u64>,
        pre_shutdown_timeout_millis: u64,
        env_filter: Option<&str>,
    ) -> PyResult<Self> {
        Ok(Self {
            span_limits: span_limits.unwrap_or_default(),
            resource: resource.unwrap_or_default(),
            metadata_map: metadata_map.map(PyAny::extract).transpose()?,
            sampler: sampler.map(PyAny::extract).transpose()?.unwrap_or_default(),
            endpoint: endpoint.map(String::from),
            timeout_millis,
            pre_shutdown_timeout_millis,
            env_filter: env_filter.map(String::from),
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

/// The Rust `OpenTelemetry` SDK does not support the official OTLP headers environment variables.
/// Here we include a custom implementation.
// https://opentelemetry.io/docs/specs/otel/protocol/exporter/
const OTEL_EXPORTER_OTLP_HEADERS: &str = "OTEL_EXPORTER_OTLP_HEADERS";
const OTEL_EXPORTER_OTLP_TRACES_HEADERS: &str = "OTEL_EXPORTER_OTLP_TRACES_HEADERS";

fn get_metadata_from_environment() -> Result<tonic::metadata::MetadataMap, ConfigError> {
    [
        OTEL_EXPORTER_OTLP_HEADERS,
        OTEL_EXPORTER_OTLP_TRACES_HEADERS,
    ]
    .iter()
    .filter_map(|k| std::env::var(k).ok())
    .flat_map(|headers| {
        headers
            .split(',')
            .map(String::from)
            .filter_map(|kv| {
                let mut x = kv.split('=').map(String::from);
                Some((x.next()?, x.next()?))
            })
            .collect::<Vec<(String, String)>>()
    })
    .try_fold(
        tonic::metadata::MetadataMap::new(),
        |mut metadata, (k, v)| {
            let key = k.parse::<MetadataKey<_>>().map_err(ConfigError::from)?;
            metadata.insert(key, v.parse().map_err(ConfigError::from)?);
            Ok(metadata)
        },
    )
}

impl TryFrom<PyConfig> for Config {
    type Error = BuildError;

    fn try_from(config: PyConfig) -> Result<Self, Self::Error> {
        let env_metadata_map = get_metadata_from_environment()?;
        let metadata_map = match config.metadata_map {
            Some(m) => Some(m.into_iter().try_fold(
                env_metadata_map,
                |mut metadata_map: tonic::metadata::MetadataMap,
                 (k, v)|
                 -> Result<_, Self::Error> {
                    let key = k.parse::<MetadataKey<_>>().map_err(ConfigError::from)?;
                    metadata_map.insert(key, v.parse().map_err(ConfigError::from)?);
                    Ok(metadata_map)
                },
            )?),
            None if !env_metadata_map.is_empty() => Some(env_metadata_map),
            None => None,
        };

        Ok(Self {
            span_limits: config.span_limits.into(),
            resource: config.resource.into(),
            metadata_map,
            sampler: config.sampler.into(),
            endpoint: config.endpoint,
            timeout: config.timeout_millis.map(Duration::from_millis),
            pre_shutdown_timeout: Duration::from_millis(config.pre_shutdown_timeout_millis),
            env_filter: config.env_filter,
        })
    }
}

create_init_submodule! {
    classes: [ PyConfig, PySpanLimits, PyResource ],
}
