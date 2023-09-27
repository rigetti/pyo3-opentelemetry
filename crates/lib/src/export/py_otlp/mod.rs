use opentelemetry_api::trace::{TraceError, TracerProvider};
use opentelemetry_api::ExportError;
use opentelemetry_proto::tonic::trace::v1::ResourceSpans;
use opentelemetry_sdk::export::trace::{ExportResult, SpanData};
use pyo3::prelude::*;

use pyo3::types::PyBytes;
use rigetti_pyo3::{create_init_submodule, ToPythonError};

#[derive(thiserror::Error, Debug)]
enum PythonExportError {
    #[error("export to Python serialization failed: {0}")]
    SerializationError(PyErr),
    #[error("error while exporting to Python: {0}")]
    ExportError(PyErr),
}

impl ExportError for PythonExportError {
    fn exporter_name(&self) -> &'static str {
        "PythonOTLPExporter"
    }
}

#[derive(Debug)]
struct PythonOTLPExporterWrapper {
    exporter: Py<PyAny>,
    wg: super::util::wg::WaitGroup,
}

impl PythonOTLPExporterWrapper {
    fn new(exporter: Py<PyAny>) -> Self {
        Self {
            exporter,
            wg: super::util::wg::WaitGroup::new(0),
        }
    }
}

fn export_to_python(
    batch: Vec<SpanData>,
    exporter: &Py<PyAny>,
    wg: &super::util::wg::WaitGroup,
) -> Result<(), PythonExportError> {
    use prost::Message;
    let resource_spans = batch.into_iter().map(ResourceSpans::from);

    Python::with_gil(|py| -> Result<(), PythonExportError> {
        let resource_spans: Result<Vec<_>, PyErr> = resource_spans
            .map(|span| {
                let span_bytes = span.encode_to_vec();
                PyBytes::new_with(py, span_bytes.len(), |bytes: &mut [u8]| {
                    bytes.copy_from_slice(span_bytes.as_slice());
                    Ok(())
                })
            })
            .collect();

        let resource_spans = resource_spans.map_err(PythonExportError::SerializationError)?;
        exporter
            .as_ref(py)
            .call_method1("export", (resource_spans,))
            .map_err(PythonExportError::ExportError)?;
        wg.done();
        Ok(())
    })
}

impl opentelemetry_sdk::export::trace::SpanExporter for PythonOTLPExporterWrapper {
    fn force_flush(&mut self) -> futures_core::future::BoxFuture<'static, ExportResult> {
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
            export_to_python(batch, &exporter, &wg)
                .map_err(|e| TraceError::ExportFailed(Box::new(e)))
        })
    }
}

#[pyclass]
#[derive(Debug)]
pub(super) struct PythonOTLPExporter {
    exporter: Py<PyAny>,
}

#[pymethods]
impl PythonOTLPExporter {
    #[new]
    fn new(exporter: Py<PyAny>) -> Self {
        Self { exporter }
    }

    fn __aenter__(&self) -> PyResult<()> {
        let exporter = PythonOTLPExporterWrapper::new(self.exporter.clone());
        super::util::start_tracer(|| {
            let provider = opentelemetry_sdk::trace::TracerProvider::builder()
                .with_batch_exporter(exporter, opentelemetry::runtime::TokioCurrentThread)
                .build();
            let tracer = provider.tracer("py-otlp");
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
    classes: [ PythonOTLPExporter ],
}
