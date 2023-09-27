use opentelemetry_api::trace::TracerProvider;
use pyo3::prelude::*;
use rigetti_pyo3::{create_init_submodule, ToPythonError};

#[pyclass]
pub(super) struct StdoutExporter;

#[pymethods]
impl StdoutExporter {
    #[new]
    const fn new() -> Self {
        Self {}
    }

    #[staticmethod]
    fn __aenter__() -> PyResult<()> {
        super::util::start_tracer(|| {
            let provider = opentelemetry_sdk::trace::TracerProvider::builder()
                .with_batch_exporter(
                    opentelemetry_stdout::SpanExporter::default(),
                    opentelemetry::runtime::TokioCurrentThread,
                )
                .build();
            let tracer = provider.tracer("stdout");
            Ok((provider, tracer))
        })
        .map_err(super::util::trace::TracerInitializationError::to_py_err)
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
    classes: [ StdoutExporter ],
}
