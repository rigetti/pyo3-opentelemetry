use opentelemetry_api::trace::TracerProvider;
use opentelemetry_proto::tonic::trace::v1::ResourceSpans;
use opentelemetry_sdk::export::trace::{ExportResult, SpanData};
use pyo3::prelude::*;

use pyo3::types::PyBytes;

#[derive(Debug)]
struct PythonOTLPExporterWrapper {
    exporter: Py<PyAny>,
    wg: super::util::WaitGroup,
}

impl PythonOTLPExporterWrapper {
    fn new(exporter: Py<PyAny>) -> Self {
        Self {
            exporter,
            wg: super::util::WaitGroup::new(0),
        }
    }
}

fn export_to_python(batch: Vec<SpanData>, exporter: Py<PyAny>, wg: super::util::WaitGroup) {
    use prost::Message;
    let resource_spans = batch.into_iter().map(ResourceSpans::from);

    Python::with_gil(|py| {
        let resource_spans = resource_spans
            .map(|span| {
                let span_bytes = span.encode_to_vec();
                let py_bytes = PyBytes::new_with(py, span_bytes.len(), |bytes: &mut [u8]| {
                    bytes.copy_from_slice(span_bytes.as_slice());
                    Ok(())
                });
                py_bytes.unwrap()
            })
            .collect::<Vec<_>>()
            .into_py(py);

        exporter
            .as_ref(py)
            .call_method1("export", (resource_spans,))
            .unwrap();
        wg.done();
    });
}

impl opentelemetry_sdk::export::trace::SpanExporter for PythonOTLPExporterWrapper {
    fn force_flush(&mut self) -> futures_core::future::BoxFuture<'static, ExportResult> {
        println!("force flush");
        let wg = self.wg.clone();
        Box::pin(async move {
            wg.wait().await;
            Ok(())
        })
    }

    fn export(
        &mut self,
        batch: Vec<SpanData>,
    ) -> futures_core::future::BoxFuture<'static, ExportResult> {
        self.wg.add(1);
        let exporter = self.exporter.clone();
        let wg = self.wg.clone();
        Box::pin(async move {
            export_to_python(batch, exporter.clone(), wg.clone());
            Ok(())
        })
    }
}

#[pyclass]
#[derive(Debug)]
pub struct PythonOTLPExporter {
    exporter: Py<PyAny>,
}

#[pymethods]
impl PythonOTLPExporter {
    #[new]
    fn new(exporter: Py<PyAny>) -> Self {
        Self { exporter }
    }

    fn __aenter__<'a>(&'a self, py: Python<'a>) -> PyResult<&PyAny> {
        let exporter = PythonOTLPExporterWrapper::new(self.exporter.clone());
        pyo3_asyncio::tokio::future_into_py(py, async move {
            super::util::start_tracer(|| {
                let provider = opentelemetry_sdk::trace::TracerProvider::builder()
                    .with_batch_exporter(exporter, opentelemetry::runtime::TokioCurrentThread)
                    .build();
                let tracer = provider.tracer("py-otlp");
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
