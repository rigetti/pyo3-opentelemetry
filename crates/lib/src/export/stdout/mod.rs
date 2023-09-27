use std::time::Duration;

use opentelemetry_api::trace::TracerProvider;
use pyo3::prelude::*;
use rigetti_pyo3::{create_init_submodule, ToPythonError};

#[derive(thiserror::Error, Debug)]
pub(crate) enum TracerInitializationError {
    #[error("failed to initialize stdout span exporter for specified file path: {0}")]
    InvalidFile(#[from] std::io::Error),
}

#[pyclass]
pub(super) struct StdoutAsyncContextManager {
    file_path: Option<String>,
    timeout_millis: u64,
}

#[pymethods]
impl StdoutAsyncContextManager {
    #[new]
    #[pyo3(signature = (file_path = None, timeout_millis = 300))]
    const fn new(file_path: Option<String>, timeout_millis: u64) -> Self {
        Self {
            file_path,
            timeout_millis,
        }
    }

    fn __aenter__(&self) -> PyResult<()> {
        let file_path = self.file_path.clone();
        let timeout = Duration::from_millis(self.timeout_millis);
        super::common::start_tracer(
            move || -> Result<_, super::common::trace::TracerInitializationError> {
                let exporter_builder = opentelemetry_stdout::SpanExporter::builder();
                let exporter_builder = match file_path {
                    Some(file_path) => {
                        let file = std::fs::File::create(file_path)
                            .map_err(TracerInitializationError::from)?;
                        exporter_builder.with_writer(file)
                    }
                    None => exporter_builder,
                };
                let provider = opentelemetry_sdk::trace::TracerProvider::builder()
                    .with_batch_exporter(
                        exporter_builder.build(),
                        opentelemetry::runtime::TokioCurrentThread,
                    )
                    .build();
                let tracer = provider.tracer("stdout");
                Ok((provider, tracer))
            },
            timeout,
        )
        .map_err(super::common::trace::TracerInitializationError::to_py_err)
    }

    #[staticmethod]
    fn __aexit__<'a>(
        py: Python<'a>,
        _exc_type: Option<&PyAny>,
        _exc_value: Option<&PyAny>,
        _traceback: Option<&PyAny>,
    ) -> PyResult<&'a PyAny> {
        super::common::stop(py)
    }
}

create_init_submodule! {
    classes: [ StdoutAsyncContextManager ],
}
