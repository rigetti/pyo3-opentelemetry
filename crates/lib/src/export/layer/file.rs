use opentelemetry_api::trace::TracerProvider;
use pyo3::prelude::*;
use rigetti_pyo3::create_init_submodule;

use super::{force_flush_provider_as_shutdown, LayerBuildResult, LayerWithShutdown};

#[pyclass]
#[derive(Clone)]
pub(crate) struct Config {
    file_path: Option<String>,
}

#[pymethods]
impl Config {
    #[new]
    #[pyo3(signature = (file_path = None))]
    const fn new(file_path: Option<String>) -> Self {
        Self { file_path }
    }
}

impl crate::export::layer::Config for Config {
    fn build(&self, batch: bool) -> LayerBuildResult<LayerWithShutdown> {
        let exporter_builder = opentelemetry_stdout::SpanExporter::builder();
        let exporter_builder = match self.file_path.as_ref() {
            Some(file_path) => {
                let file = std::fs::File::create(file_path).map_err(BuildError::from)?;
                exporter_builder.with_writer(file)
            }
            None => exporter_builder,
        };
        let provider = if batch {
            opentelemetry_sdk::trace::TracerProvider::builder()
                .with_batch_exporter(
                    exporter_builder.build(),
                    opentelemetry::runtime::TokioCurrentThread,
                )
                .build()
        } else {
            opentelemetry_sdk::trace::TracerProvider::builder()
                .with_simple_exporter(exporter_builder.build())
                .build()
        };
        let tracer = provider.tracer("stdout");
        let layer = tracing_opentelemetry::layer().with_tracer(tracer);
        Ok(LayerWithShutdown {
            layer: Box::new(layer),
            shutdown: force_flush_provider_as_shutdown(provider),
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum BuildError {
    #[error("failed to initialize file span exporter for specified file path: {0}")]
    InvalidFile(#[from] std::io::Error),
}

create_init_submodule! {
    classes: [ Config ],
}
