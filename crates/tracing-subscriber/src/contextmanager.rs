// Copyright 2023 Rigetti Computing
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use std::fmt::Debug;

use pyo3::{exceptions::PyRuntimeError, prelude::*};

use rigetti_pyo3::{py_wrap_error, wrap_error, ToPythonError};

use super::export_process::{
    ExportProcess, ExportProcessConfig, RustTracingShutdownError, RustTracingStartError,
};

#[pyclass]
#[derive(Clone, Debug, Default)]
pub(crate) struct GlobalTracingConfig {
    pub(crate) export_process: ExportProcessConfig,
}

#[pymethods]
impl GlobalTracingConfig {
    #[new]
    #[pyo3(signature = (/, export_process = None))]
    #[allow(clippy::pedantic)]
    fn new(export_process: Option<ExportProcessConfig>) -> PyResult<Self> {
        let export_process = export_process.unwrap_or_default();
        Ok(Self { export_process })
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub(crate) struct CurrentThreadTracingConfig {
    pub(crate) export_process: ExportProcessConfig,
}

#[pymethods]
impl CurrentThreadTracingConfig {
    #[new]
    #[pyo3(signature = (/, export_process = None))]
    #[allow(clippy::pedantic)]
    fn new(export_process: Option<ExportProcessConfig>) -> PyResult<Self> {
        let export_process = export_process.unwrap_or_default();
        Ok(Self { export_process })
    }
}

#[derive(FromPyObject, Debug)]
pub(crate) enum TracingConfig {
    Global(GlobalTracingConfig),
    CurrentThread(CurrentThreadTracingConfig),
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self::Global(GlobalTracingConfig::default())
    }
}

/// Represents the current state of the context manager. This state is used to ensure the
/// context manager methods are invoked in the correct order and multiplicity.
#[derive(Debug)]
enum ContextManagerState {
    Initialized(TracingConfig),
    Entered(ExportProcess),
    Starting,
    Exited,
}

/// A Python class that implements the context manager interface. It is initialized with a
/// configuration. Upon entry it builds and installs the configured tracing subscriber. Upon exit
/// it shuts down the tracing subscriber.
#[pyclass]
#[derive(Debug)]
pub struct Tracing {
    state: ContextManagerState,
}

#[derive(thiserror::Error, Debug)]
enum ContextManagerError {
    #[error("entered tracing context manager with no configuration defined; ensure contextmanager only enters once")]
    EnterWithoutConfiguration,
    #[error("exited tracing context manager with no export process defined; ensure contextmanager only exits once after being entered")]
    ExitWithoutExportProcess,
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
    #[allow(clippy::pedantic)]
    fn new(config: Option<TracingConfig>) -> PyResult<Self> {
        let config = config.unwrap_or_default();
        Ok(Self {
            state: ContextManagerState::Initialized(config),
        })
    }

    fn __aenter__<'a>(&'a mut self, py: Python<'a>) -> PyResult<&'a PyAny> {
        let state = std::mem::replace(&mut self.state, ContextManagerState::Starting);
        if let ContextManagerState::Initialized(config) = state {
            self.state = ContextManagerState::Entered(
                ExportProcess::start(config)
                    .map_err(RustTracingStartError::from)
                    .map_err(ToPythonError::to_py_err)?,
            );
        } else {
            return Err(ContextManagerError::EnterWithoutConfiguration)
                .map_err(RustContextManagerError::from)
                .map_err(ToPythonError::to_py_err)?;
        }

        pyo3_asyncio::tokio::future_into_py(py, async { Ok(()) })
    }

    fn __aexit__<'a>(
        &'a mut self,
        py: Python<'a>,
        _exc_type: Option<&PyAny>,
        _exc_value: Option<&PyAny>,
        _traceback: Option<&PyAny>,
    ) -> PyResult<&'a PyAny> {
        let state = std::mem::replace(&mut self.state, ContextManagerState::Exited);
        if let ContextManagerState::Entered(export_process) = state {
            let py_rt = pyo3_asyncio::tokio::get_runtime();
            let export_runtime = py_rt.block_on(async move {
                export_process
                    .shutdown()
                    .await
                    .map_err(RustTracingShutdownError::from)
                    .map_err(ToPythonError::to_py_err)
            })?;
            if let Some(export_runtime) = export_runtime {
                // This immediately shuts the runtime down. The expectation here is that the
                // process shutdown is responsible for cleaning up all background tasks and
                // shutting down gracefully.
                export_runtime.shutdown_background();
            }
        } else {
            return Err(ContextManagerError::ExitWithoutExportProcess)
                .map_err(RustContextManagerError::from)
                .map_err(ToPythonError::to_py_err)?;
        }

        pyo3_asyncio::tokio::future_into_py(py, async { Ok(()) })
    }
}

#[cfg(feature = "layer-otel-otlp-file")]
#[cfg(test)]
mod test {
    use std::{io::BufRead, thread::sleep, time::Duration};

    use tokio::runtime::Builder;

    use crate::{
        contextmanager::{CurrentThreadTracingConfig, GlobalTracingConfig, TracingConfig},
        export_process::{BatchConfig, ExportProcess, ExportProcessConfig, SimpleConfig},
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
    /// Test that a global batch process can be started and stopped and that it exports
    /// accurate spans as configured.
    fn test_global_batch() {
        let temporary_file = tempfile::NamedTempFile::new().unwrap();
        let temporary_file_path = temporary_file.path().to_owned();
        let layer_config = Box::new(crate::layers::otel_otlp_file::Config {
            file_path: Some(temporary_file_path.as_os_str().to_str().unwrap().to_owned()),
            filter: Some("error,pyo3_tracing_subscriber=info".to_string()),
        });
        let subscriber = Box::new(TracingSubscriberRegistryConfig { layer_config });
        let config = TracingConfig::Global(GlobalTracingConfig {
            export_process: ExportProcessConfig::Batch(BatchConfig {
                subscriber: crate::subscriber::PyConfig {
                    subscriber_config: subscriber,
                },
            }),
        });
        let export_process = ExportProcess::start(config).unwrap();
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

        let span_grace = Duration::from_millis(200);
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
    /// Test that a global simple export process can be started and stopped and that it
    /// exports accurate spans as configured.
    fn test_global_simple() {
        let temporary_file = tempfile::NamedTempFile::new().unwrap();
        let temporary_file_path = temporary_file.path().to_owned();
        let layer_config = Box::new(crate::layers::otel_otlp_file::Config {
            file_path: Some(temporary_file_path.as_os_str().to_str().unwrap().to_owned()),
            filter: Some("error,pyo3_tracing_subscriber=info".to_string()),
        });
        let subscriber = Box::new(TracingSubscriberRegistryConfig { layer_config });
        let config = TracingConfig::Global(GlobalTracingConfig {
            export_process: ExportProcessConfig::Simple(SimpleConfig {
                subscriber: crate::subscriber::PyConfig {
                    subscriber_config: subscriber,
                },
            }),
        });
        let export_process = ExportProcess::start(config).unwrap();
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
    /// Test that a current thread simple export process can be started and stopped and that it
    /// exports accurate spans as configured.
    fn test_current_thread_simple() {
        let temporary_file = tempfile::NamedTempFile::new().unwrap();
        let temporary_file_path = temporary_file.path().to_owned();
        let layer_config = Box::new(crate::layers::otel_otlp_file::Config {
            file_path: Some(temporary_file_path.as_os_str().to_str().unwrap().to_owned()),
            filter: Some("error,pyo3_tracing_subscriber=info".to_string()),
        });
        let subscriber = Box::new(TracingSubscriberRegistryConfig { layer_config });
        let config = TracingConfig::CurrentThread(CurrentThreadTracingConfig {
            export_process: crate::export_process::ExportProcessConfig::Simple(SimpleConfig {
                subscriber: crate::subscriber::PyConfig {
                    subscriber_config: subscriber,
                },
            }),
        });
        let export_process = ExportProcess::start(config).unwrap();

        for _ in 0..N_SPANS {
            example();
        }

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
