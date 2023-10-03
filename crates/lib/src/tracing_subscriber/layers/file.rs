use std::path::Path;

use opentelemetry_api::trace::TracerProvider;
use pyo3::prelude::*;
use rigetti_pyo3::create_init_submodule;

use super::{force_flush_provider_as_shutdown, LayerBuildResult, WithShutdown};

#[pyclass]
#[derive(Clone, Debug, Default)]
pub(crate) struct Config {
    pub(crate) file_path: Option<String>,
}

#[pymethods]
impl Config {
    #[new]
    #[pyo3(signature = (/, file_path = None))]
    const fn new(file_path: Option<String>) -> Self {
        Self { file_path }
    }
}

impl crate::tracing_subscriber::layers::Config for Config {
    fn build(&self, batch: bool) -> LayerBuildResult<WithShutdown> {
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
        Ok(WithShutdown {
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

pub(super) fn build_stub_files(directory: &Path) -> Result<(), std::io::Error> {
    let data = include_bytes!("../../../assets/python_stubs/layers/file/__init__.pyi");
    std::fs::create_dir_all(directory)?;
    let init_file = directory.join("__init__.pyi");
    std::fs::write(init_file, data)
}
