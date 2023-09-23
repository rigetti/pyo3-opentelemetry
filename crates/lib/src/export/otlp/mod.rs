// use std::time::Duration;

use opentelemetry_otlp::WithExportConfig;
// use opentelemetry_sdk::Resource;
use pyo3::prelude::*;

use opentelemetry_sdk::trace::{self};

#[pyclass]
#[derive(Clone)]
struct Config {
    // span_limits: opentelemetry_sdk::trace::SpanLimits,
    // resource: Resource,
    // metadata_map: tonic::metadata::MetadataMap,
    // sampler: Sampler,
    // endpoint: Option<String>,
    // timeout: Option<Duration>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // span_limits: opentelemetry_sdk::trace::SpanLimits::default(),
            // resource: Resource::default(),
            // metadata_map: tonic::metadata::MetadataMap::default(),
            // sampler: Sampler::AlwaysOn,
            // endpoint: None,
            // timeout: None,
        }
    }
}

#[pyclass]
struct OTLPExporter {
    config: Config,
}

#[pymethods]
impl OTLPExporter {
    #[new]
    fn new(config: Config) -> Self {
        Self { config }
    }

    fn __aenter__<'a>(&'a self, py: Python<'a>) -> PyResult<&PyAny> {
        // let config = self.config.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            super::util::start_tracer(|| {
                let otlp_exporter = opentelemetry_otlp::new_exporter().tonic().with_env();
                // Then pass it into pipeline builder
                let tracer = opentelemetry_otlp::new_pipeline()
                    .tracing()
                    .with_exporter(otlp_exporter)
                    .with_trace_config(
                        trace::config(), // .with_sampler(config.sampler)
                                         // .with_span_limits(config.span_limits)
                                         // .with_resource(config.resource),
                    )
                    .install_batch(opentelemetry::runtime::TokioCurrentThread)
                    .unwrap();
                let provider = tracer.provider().unwrap();
                (provider, tracer)
            });

            Ok(())
        })
    }

    fn __aexit__<'a>(
        &'a mut self,
        py: Python<'a>,
        _exc_type: Option<&PyAny>,
        _exc_value: Option<&PyAny>,
        _traceback: Option<&PyAny>,
    ) -> PyResult<&PyAny> {
        super::util::stop(py)
    }
}
