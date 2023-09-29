use std::time::Duration;

use crate::export::subscriber::PyConfig;
use pyo3::{exceptions::PyRuntimeError, prelude::*};
use rigetti_pyo3::{create_init_submodule, py_wrap_error, wrap_error};
use tracing::subscriber::SetGlobalDefaultError;

use super::contextmanager::TracingConfig;

mod current_thread_simple;
mod global_batch;
mod global_simple;

#[pyclass]
#[derive(Clone)]
struct BatchConfig {
    subscriber: PyConfig,
    timeout_millis: u64,
}

#[pyclass]
#[derive(Clone)]
struct SimpleConfig {
    subscriber: PyConfig,
}

#[derive(FromPyObject, Clone)]
pub enum ExportProcessConfig {
    Batch(BatchConfig),
    Simple(SimpleConfig),
}

pub enum ExportProcess {
    GlobalBatch(global_batch::ExportProcess),
    GlobalSimple(global_simple::ExportProcess),
    CurrentThreadSimple(current_thread_simple::ExportProcess),
}

#[derive(thiserror::Error, Debug)]
enum ConfigError {
    #[error("global batch export: {0}")]
    GlobalBatchInitialization(#[from] global_batch::InitializationError),
    #[error("failed to build subscriber: {0}")]
    SubscriberBuild(#[from] crate::export::subscriber::BuildError),
}

wrap_error!(RustTracingConfigError(ConfigError));
py_wrap_error!(
    export_process,
    RustTracingConfigError,
    TracingConfigurationError,
    PyRuntimeError
);

#[derive(thiserror::Error, Debug)]
enum StartError {
    #[error("failed to start global batch")]
    GlobalBatch(#[from] global_batch::StartError),
    #[error("failed to set global default tracing subscriber")]
    SetSubscriber(#[from] SetGlobalDefaultError),
}

wrap_error!(RustTracingStartError(StartError));
py_wrap_error!(
    export_process,
    RustTracingStartError,
    TracingStartError,
    PyRuntimeError
);

type StartResult<T> = Result<T, StartError>;

#[derive(thiserror::Error, Debug)]
enum ShutdownError {
    #[error("the subscriber failed to shutdown")]
    Subscriber(#[from] crate::export::subscriber::ShutdownError),
}

wrap_error!(RustTracingShutdownError(ShutdownError));
py_wrap_error!(
    export_process,
    RustTracingShutdownError,
    TracingShutdownError,
    PyRuntimeError
);

type ShutdownResult<T> = Result<T, ShutdownError>;

impl TryFrom<TracingConfig> for ExportProcess {
    type Error = ConfigError;

    fn try_from(config: TracingConfig) -> Result<Self, ConfigError> {
        match config {
            TracingConfig::Global(config) => match config.export_process {
                ExportProcessConfig::Batch(config) => Ok(ExportProcess::GlobalBatch(
                    global_batch::ExportProcess::new(
                        config.subscriber.subscriber_config,
                        Duration::from_millis(config.timeout_millis),
                    )?,
                )),
                ExportProcessConfig::Simple(config) => {
                    let subscriber = config.subscriber.subscriber_config.build(false)?;
                    let process = global_simple::ExportProcess::new(subscriber);
                    Ok(ExportProcess::GlobalSimple(process))
                }
            },
            TracingConfig::CurrentThread(config) => {
                let subscriber = config.subscriber.subscriber_config.build(false)?;
                let process = current_thread_simple::ExportProcess::new(subscriber);
                Ok(ExportProcess::CurrentThreadSimple(process))
            }
        }
    }
}

impl ExportProcess {
    pub(crate) fn start(&mut self) -> StartResult<()> {
        match self {
            ExportProcess::GlobalBatch(process) => Ok(process.start_tracer()?),
            ExportProcess::GlobalSimple(process) => process.start_tracer(),
            ExportProcess::CurrentThreadSimple(process) => Ok(process.start_tracer()),
        }
    }

    pub(crate) async fn shutdown(self) -> ShutdownResult<()> {
        match self {
            ExportProcess::GlobalBatch(process) => {
                process.shutdown().await;
                Ok(())
            }
            ExportProcess::GlobalSimple(process) => process.shutdown().await,
            ExportProcess::CurrentThreadSimple(process) => process.shutdown().await,
        }
    }
}

create_init_submodule! {
    errors: [TracingStartError, TracingShutdownError, TracingConfigurationError],
}

#[cfg(TODO)]
mod test {
    use std::{io::BufRead, thread::sleep, time::Duration};

    use opentelemetry_api::trace::TracerProvider;
    use tokio::runtime::Builder;
    use tracing_opentelemetry::OpenTelemetryLayer;
    use tracing_subscriber::{layer::Layered, Registry};

    #[tracing::instrument]
    fn example() {
        sleep(SPAN_DURATION);
    }

    const N_SPANS: usize = 5;
    const SPAN_DURATION: Duration = Duration::from_millis(100);

    /// A function that can be passed to `BatchExportProcess::start_tracer` and
    /// executes the instrumented `example` function `N_SPANS` times.
    fn set_subscriber1(
        subscriber: Layered<
            OpenTelemetryLayer<Registry, opentelemetry_sdk::trace::Tracer>,
            Registry,
        >,
    ) {
        tracing::subscriber::with_default(subscriber, || {
            for _ in 0..N_SPANS {
                example();
            }
        });
    }

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
    fn test_start_and_stop_batch_export_process() {
        let batch_export_process = super::BatchExportProcess::new().unwrap();
        let temporary_file = tempfile::NamedTempFile::new().unwrap();
        let temporary_file_path = temporary_file.path().to_owned();
        batch_export_process
            .start_tracer(
                || {
                    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
                        .with_batch_exporter(
                            opentelemetry_stdout::SpanExporter::builder()
                                .with_writer(temporary_file)
                                .build(),
                            opentelemetry::runtime::TokioCurrentThread,
                        )
                        .build();
                    let tracer = provider.tracer("stdout");

                    Ok((provider, tracer))
                },
                |subscriber| {
                    set_subscriber1(subscriber);
                    Ok(())
                },
                Duration::from_secs(5),
            )
            .unwrap();

        let rt2 = Builder::new_current_thread().enable_time().build().unwrap();
        let _guard = rt2.enter();
        rt2.block_on(tokio::time::timeout(
            Duration::from_secs(1),
            batch_export_process.shutdown(),
        ))
        .unwrap();

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
