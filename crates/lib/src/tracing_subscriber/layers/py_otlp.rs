use std::path::Path;

use crate::tracing_subscriber::common::wg;
use opentelemetry_api::{
    trace::{TraceError, TracerProvider},
    ExportError,
};
use opentelemetry_proto::tonic::trace::v1::ResourceSpans;
use opentelemetry_sdk::export::trace::{ExportResult, SpanData};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use rigetti_pyo3::create_init_submodule;

use super::{force_flush_provider_as_shutdown, LayerBuildResult, WithShutdown};

#[pyclass]
#[derive(Clone, Debug)]
pub(crate) struct Config {
    exporter: Py<PyAny>,
}

#[pymethods]
impl Config {
    #[new]
    #[pyo3(signature = (/, exporter))]
    fn new(exporter: Py<PyAny>) -> Self {
        Self { exporter }
    }
}

impl crate::tracing_subscriber::layers::Config for Config {
    fn build(&self, batch: bool) -> LayerBuildResult<WithShutdown> {
        let provider = if batch {
            opentelemetry_sdk::trace::TracerProvider::builder()
                .with_batch_exporter(
                    PythonOTLPSpanExporter::new(self.exporter.clone()),
                    opentelemetry::runtime::TokioCurrentThread,
                )
                .build()
        } else {
            opentelemetry_sdk::trace::TracerProvider::builder()
                .with_simple_exporter(PythonOTLPSpanExporter::new(self.exporter.clone()))
                .build()
        };
        let tracer = provider.tracer("py-otlp");
        let layer = tracing_opentelemetry::layer().with_tracer(tracer);
        Ok(WithShutdown {
            layer: Box::new(layer),
            shutdown: force_flush_provider_as_shutdown(provider),
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    #[error("export to Python serialization failed: {0}")]
    SerializationError(PyErr),
    #[error("error while exporting to Python: {0}")]
    ExportError(PyErr),
}

impl ExportError for Error {
    fn exporter_name(&self) -> &'static str {
        "pyo3-opentelemetry::export::layer::py-otlp"
    }
}

#[derive(Debug)]
struct PythonOTLPSpanExporter {
    exporter: Py<PyAny>,
    wg: wg::WaitGroup,
}

impl PythonOTLPSpanExporter {
    fn new(exporter: Py<PyAny>) -> Self {
        Self {
            exporter,
            wg: wg::WaitGroup::new(0),
        }
    }
}

fn export_to_python(
    batch: Vec<SpanData>,
    exporter: &Py<PyAny>,
    wg: &wg::WaitGroup,
) -> Result<(), Error> {
    use prost::Message;
    let resource_spans = batch.into_iter().map(ResourceSpans::from);

    Python::with_gil(|py| -> Result<(), Error> {
        let resource_spans: Result<Vec<_>, PyErr> = resource_spans
            .map(|span| {
                let span_bytes = span.encode_to_vec();
                PyBytes::new_with(py, span_bytes.len(), |bytes: &mut [u8]| {
                    bytes.copy_from_slice(span_bytes.as_slice());
                    Ok(())
                })
            })
            .collect();

        let resource_spans = resource_spans.map_err(Error::SerializationError)?;
        exporter
            .as_ref(py)
            .call_method1("export", (resource_spans,))
            .map_err(Error::ExportError)?;
        wg.done();
        Ok(())
    })
}

impl opentelemetry_sdk::export::trace::SpanExporter for PythonOTLPSpanExporter {
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

create_init_submodule! {
    classes: [ Config ],
}

#[allow(dead_code)]
pub(super) fn build_stub_files(directory: &Path) -> Result<(), std::io::Error> {
    let data = include_bytes!("../../../assets/python_stubs/layers/py_otlp/__init__.pyi");
    std::fs::create_dir_all(directory)?;
    let init_file = directory.join("__init__.pyi");
    std::fs::write(init_file, data)
}
