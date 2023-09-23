use opentelemetry_api::trace::TracerProvider;
use pyo3::prelude::*;

#[pyclass]
struct StdoutExporter;

#[pymethods]
impl StdoutExporter {
    #[new]
    fn new() -> Self {
        Self {}
    }

    fn __aenter__<'a>(&'a mut self, py: Python<'a>) -> PyResult<&PyAny> {
        pyo3_asyncio::tokio::future_into_py(py, async move {
            super::util::start_tracer(|| {
                let provider = opentelemetry_sdk::trace::TracerProvider::builder()
                    .with_batch_exporter(
                        opentelemetry_stdout::SpanExporter::default(),
                        opentelemetry::runtime::TokioCurrentThread,
                    )
                    .build();
                let tracer = provider.tracer("stdout");
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
