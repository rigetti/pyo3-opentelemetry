use std::fmt::Debug;

use pyo3::{exceptions::PyRuntimeError, prelude::*};

use rigetti_pyo3::{py_wrap_error, wrap_error, ToPythonError};

use super::export_process::{
    ExportProcess, ExportProcessConfig, RustTracingInitializationError, RustTracingShutdownError,
    RustTracingStartError,
};

#[pyclass]
#[derive(Clone)]
#[cfg_attr(any(feature = "export-file", feature = "export-otlp"), derive(Default))]
pub(crate) struct GlobalTracingConfig {
    pub(crate) export_process: ExportProcessConfig,
}

#[pymethods]
impl GlobalTracingConfig {
    #[new]
    #[pyo3(signature = (/, export_process = None))]
    #[allow(clippy::pedantic)]
    fn new(export_process: Option<ExportProcessConfig>) -> PyResult<Self> {
        #[cfg(any(feature = "export-file", feature = "export-otlp"))]
        let export_process = export_process.unwrap_or_default();
        #[cfg(all(not(feature = "export-file"), not(feature = "export-otlp")))]
        let export_process =
            crate::tracing_subscriber::unsupported_default_initialization(export_process)?;
        Ok(Self { export_process })
    }
}

#[pyclass]
#[derive(Clone)]
pub(crate) struct CurrentThreadTracingConfig {
    pub(crate) export_process: ExportProcessConfig,
}

#[pymethods]
impl CurrentThreadTracingConfig {
    #[new]
    #[pyo3(signature = (/, export_process = None))]
    #[allow(clippy::pedantic)]
    fn new(export_process: Option<ExportProcessConfig>) -> PyResult<Self> {
        #[cfg(any(feature = "export-file", feature = "export-otlp"))]
        let export_process = export_process.unwrap_or_default();
        #[cfg(all(not(feature = "export-file"), not(feature = "export-otlp")))]
        let export_process = export_process
            .ok_or(crate::tracing_subscriber::export_process::InitializationError::NoDefaultInitialization)
            .map_err(crate::tracing_subscriber::export_process::RustTracingInitializationError::from)
            .map_err(rigetti_pyo3::ToPythonError::to_py_err)?;
        Ok(Self { export_process })
    }
}

#[derive(FromPyObject)]
pub(crate) enum TracingConfig {
    Global(GlobalTracingConfig),
    CurrentThread(CurrentThreadTracingConfig),
}

#[cfg(any(feature = "export-file", feature = "export-otlp"))]
impl Default for TracingConfig {
    fn default() -> Self {
        Self::Global(GlobalTracingConfig::default())
    }
}

#[pyclass]
pub struct Tracing {
    export_process: Option<ExportProcess>,
}

impl Debug for Tracing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tracing {{ export_process: {} }}",
            self.export_process.as_ref().map_or("None", |_| "Some(_)")
        )
    }
}

#[derive(thiserror::Error, Debug)]
enum ContextManagerError {
    #[error("entered tracing context manager with no export process defined")]
    Enter,
    #[error("exited tracing context manager with no export process defined")]
    Exit,
}

wrap_error!(RustContextManagerError(ContextManagerError));
py_wrap_error!(
    contextmanager,
    RustContextManagerError,
    TracingContextManagerError,
    PyRuntimeError
);

#[pymethods]
impl Tracing {
    #[new]
    #[pyo3(signature = (/, config = None))]
    fn new(config: Option<TracingConfig>) -> PyResult<Self> {
        #[cfg(any(feature = "export-file", feature = "export-otlp"))]
        let config = config.unwrap_or_default();
        #[cfg(all(not(feature = "export-file"), not(feature = "export-otlp")))]
        let config = crate::tracing_subscriber::unsupported_default_initialization(config)?;
        let export_process = Some(
            config
                .try_into()
                .map_err(RustTracingInitializationError::from)
                .map_err(ToPythonError::to_py_err)?,
        );
        Ok(Self { export_process })
    }

    fn __aenter__(&mut self) -> PyResult<()> {
        self.export_process
            .as_mut()
            .ok_or(ContextManagerError::Enter)
            .map_err(RustContextManagerError::from)
            .map_err(ToPythonError::to_py_err)?
            .start()
            .map_err(RustTracingStartError::from)
            .map_err(ToPythonError::to_py_err)
    }

    fn __aexit__<'a>(
        &'a mut self,
        py: Python<'a>,
        _exc_type: Option<&PyAny>,
        _exc_value: Option<&PyAny>,
        _traceback: Option<&PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let export_process = self
            .export_process
            .take()
            .ok_or(ContextManagerError::Exit)
            .map_err(RustContextManagerError::from)
            .map_err(ToPythonError::to_py_err)?;
        let py_rt = pyo3_asyncio::tokio::get_runtime();
        py_rt
            .block_on(async move {
                export_process
                    .shutdown()
                    .await
                    .map_err(RustTracingShutdownError::from)
                    .map_err(ToPythonError::to_py_err)
            })
            .map(|_| py.None())
    }
}

#[cfg(test)]
mod test {
    use std::{io::BufRead, thread::sleep, time::Duration};

    use tokio::runtime::Builder;

    use crate::tracing_subscriber::{
        contextmanager::{CurrentThreadTracingConfig, GlobalTracingConfig, Tracing, TracingConfig},
        export_process::{BatchConfig, ExportProcessConfig, SimpleConfig},
        subscriber::TracingSubscriberRegistryConfig,
    };

    #[tracing::instrument]
    fn example() {
        sleep(SPAN_DURATION);
    }

    const N_SPANS: usize = 5;
    const SPAN_DURATION: Duration = Duration::from_millis(100);

    /// A truncated implementation of `opentelemetry_stdout` that derives
    /// `serde::Deserialize`.
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SpanData {
        resource_spans: Vec<ResourceSpan>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ResourceSpan {
        scope_spans: Vec<ScopeSpan>,
    }

    #[derive(serde::Deserialize)]
    struct ScopeSpan {
        spans: Vec<Span>,
    }

    #[derive(serde::Deserialize, Clone)]
    #[serde(rename_all = "camelCase")]
    struct Span {
        name: String,
        start_time_unix_nano: u128,
        end_time_unix_nano: u128,
    }

    #[test]
    /// Test that the `BatchExportProcess` can be started and stopped and that it exports
    /// accurate spans as configured.
    fn test_global_batch() {
        let temporary_file = tempfile::NamedTempFile::new().unwrap();
        let temporary_file_path = temporary_file.path().to_owned();
        let layer_config = Box::new(crate::tracing_subscriber::layers::file::Config {
            file_path: Some(temporary_file_path.as_os_str().to_str().unwrap().to_owned()),
        });
        let subscriber = Box::new(TracingSubscriberRegistryConfig { layer_config });
        let config = TracingConfig::Global(GlobalTracingConfig {
            export_process: ExportProcessConfig::Batch(BatchConfig {
                subscriber: crate::tracing_subscriber::subscriber::PyConfig {
                    subscriber_config: subscriber,
                },
                timeout_millis: 1000,
            }),
        });
        let mut tracing = Tracing::new(Some(config)).unwrap();
        tracing.__aenter__().unwrap();

        let export_process = tracing.export_process.unwrap();
        let rt2 = Builder::new_current_thread().enable_time().build().unwrap();
        let _guard = rt2.enter();
        let export_runtime = rt2
            .block_on(tokio::time::timeout(Duration::from_secs(1), async move {
                for _ in 0..N_SPANS {
                    example();
                }
                export_process.shutdown().await
            }))
            .unwrap()
            .unwrap()
            .unwrap();
        drop(export_runtime);

        let reader = std::io::BufReader::new(std::fs::File::open(temporary_file_path).unwrap());
        let lines = reader.lines();
        let spans = lines
            .flat_map(|line| {
                let line = line.unwrap();
                let span_data: SpanData = serde_json::from_str(line.as_str()).unwrap();
                span_data
                    .resource_spans
                    .iter()
                    .flat_map(|resource_span| {
                        resource_span
                            .scope_spans
                            .iter()
                            .flat_map(|scope_span| scope_span.spans.clone())
                    })
                    .collect::<Vec<Span>>()
            })
            .collect::<Vec<Span>>();
        assert_eq!(spans.len(), N_SPANS);

        let span_grace = Duration::from_millis(10);
        for span in spans {
            assert_eq!(span.name, "example");
            assert!(
                span.end_time_unix_nano - span.start_time_unix_nano >= SPAN_DURATION.as_nanos()
            );
            assert!(
                (span.end_time_unix_nano - span.start_time_unix_nano)
                    <= (SPAN_DURATION.as_nanos() + span_grace.as_nanos())
            );
        }
    }

    #[test]
    fn test_global_simple() {
        let temporary_file = tempfile::NamedTempFile::new().unwrap();
        let temporary_file_path = temporary_file.path().to_owned();
        let layer_config = Box::new(crate::tracing_subscriber::layers::file::Config {
            file_path: Some(temporary_file_path.as_os_str().to_str().unwrap().to_owned()),
        });
        let subscriber = Box::new(TracingSubscriberRegistryConfig { layer_config });
        let config = TracingConfig::Global(GlobalTracingConfig {
            export_process: ExportProcessConfig::Simple(SimpleConfig {
                subscriber: crate::tracing_subscriber::subscriber::PyConfig {
                    subscriber_config: subscriber,
                },
            }),
        });
        let mut tracing = Tracing::new(Some(config)).unwrap();
        tracing.__aenter__().unwrap();

        let export_process = tracing.export_process.unwrap();
        let rt2 = Builder::new_current_thread().enable_time().build().unwrap();
        let _guard = rt2.enter();
        let runtime = rt2
            .block_on(tokio::time::timeout(Duration::from_secs(1), async move {
                for _ in 0..N_SPANS {
                    example();
                }
                export_process.shutdown().await
            }))
            .unwrap()
            .unwrap();
        assert!(runtime.is_none());

        let reader = std::io::BufReader::new(std::fs::File::open(temporary_file_path).unwrap());
        let lines = reader.lines();
        let spans = lines
            .flat_map(|line| {
                let line = line.unwrap();
                let span_data: SpanData = serde_json::from_str(line.as_str()).unwrap();
                span_data
                    .resource_spans
                    .iter()
                    .flat_map(|resource_span| {
                        resource_span
                            .scope_spans
                            .iter()
                            .flat_map(|scope_span| scope_span.spans.clone())
                    })
                    .collect::<Vec<Span>>()
            })
            .collect::<Vec<Span>>();
        assert_eq!(spans.len(), N_SPANS);

        let span_grace = Duration::from_millis(10);
        for span in spans {
            assert_eq!(span.name, "example");
            assert!(
                span.end_time_unix_nano - span.start_time_unix_nano >= SPAN_DURATION.as_nanos()
            );
            assert!(
                (span.end_time_unix_nano - span.start_time_unix_nano)
                    <= (SPAN_DURATION.as_nanos() + span_grace.as_nanos())
            );
        }
    }

    #[test]
    fn test_current_thread_simple() {
        let temporary_file = tempfile::NamedTempFile::new().unwrap();
        let temporary_file_path = temporary_file.path().to_owned();
        let layer_config = Box::new(crate::tracing_subscriber::layers::file::Config {
            file_path: Some(temporary_file_path.as_os_str().to_str().unwrap().to_owned()),
        });
        let subscriber = Box::new(TracingSubscriberRegistryConfig { layer_config });
        let config = TracingConfig::CurrentThread(CurrentThreadTracingConfig {
            export_process: crate::tracing_subscriber::export_process::ExportProcessConfig::Simple(
                SimpleConfig {
                    subscriber: crate::tracing_subscriber::subscriber::PyConfig {
                        subscriber_config: subscriber,
                    },
                },
            ),
        });
        let mut tracing = Tracing::new(Some(config)).unwrap();
        tracing.__aenter__().unwrap();

        for _ in 0..N_SPANS {
            example();
        }

        let export_process = tracing.export_process.unwrap();

        let rt2 = Builder::new_current_thread().enable_time().build().unwrap();
        let _guard = rt2.enter();
        let runtime = rt2
            .block_on(tokio::time::timeout(Duration::from_secs(1), async move {
                export_process.shutdown().await
            }))
            .unwrap()
            .unwrap();
        assert!(runtime.is_none());

        let reader = std::io::BufReader::new(std::fs::File::open(temporary_file_path).unwrap());
        let lines = reader.lines();
        let spans = lines
            .flat_map(|line| {
                let line = line.unwrap();
                let span_data: SpanData = serde_json::from_str(line.as_str()).unwrap();
                span_data
                    .resource_spans
                    .iter()
                    .flat_map(|resource_span| {
                        resource_span
                            .scope_spans
                            .iter()
                            .flat_map(|scope_span| scope_span.spans.clone())
                    })
                    .collect::<Vec<Span>>()
            })
            .collect::<Vec<Span>>();
        assert_eq!(spans.len(), N_SPANS);

        let span_grace = Duration::from_millis(10);
        for span in spans {
            assert_eq!(span.name, "example");
            assert!(
                span.end_time_unix_nano - span.start_time_unix_nano >= SPAN_DURATION.as_nanos()
            );
            assert!(
                (span.end_time_unix_nano - span.start_time_unix_nano)
                    <= (SPAN_DURATION.as_nanos() + span_grace.as_nanos())
            );
        }
    }
}
