use std::sync::Arc;
use std::time::Duration;

use opentelemetry_sdk::trace::TracerProvider;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::Notify;
use tracing::subscriber::SetGlobalDefaultError;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::layer::{Layered, SubscriberExt};
use tracing_subscriber::Registry;

#[derive(thiserror::Error, Debug)]
pub(crate) enum TracerInitializationError {
    #[error("only one exporter can be initialized per process")]
    AlreadyInitialized,
    #[error("failed to initialize export background tokio runtime")]
    RuntimeInitialization(#[from] std::io::Error),
    #[error("failed to set global default tracing subscriber")]
    SetSubscriber(#[from] SetGlobalDefaultError),
    #[error("exporter initialization timed out")]
    ExportInitializationTimeout(#[from] tokio::time::error::Elapsed),
    #[error("failed to receive export initialization signal")]
    ExportInitializationRecv(#[from] tokio::sync::oneshot::error::RecvError),
    #[error("exporter is not initialized")]
    Uninitialized,
    #[cfg(feature = "export-otlp")]
    #[error("failed to initialize OTLP tracer provider")]
    Otlp(#[from] crate::export::otlp::TracerInitializationError),
    #[cfg(feature = "export-stdout")]
    #[error("failed to initialize stdout tracer provider")]
    Stdout(#[from] crate::export::stdout::TracerInitializationError),
}

pub(crate) struct BatchExportProcess {
    shutdown_notify: Arc<Notify>,
    runtime: Runtime,
}

impl BatchExportProcess {
    pub(super) fn new() -> TracerInitializationResult<Self> {
        let runtime = init_runtime()?;
        let shutdown_notify = Arc::new(Notify::new());
        Ok(Self {
            shutdown_notify,
            runtime,
        })
    }

    pub(super) fn start_tracer<InitTracerProvider, SetSubscriber>(
        &self,
        init_tracer_provider: InitTracerProvider,
        set_subscriber: SetSubscriber,
        to: Duration,
    ) -> TracerInitializationResult<()>
    where
        InitTracerProvider: FnOnce() -> Result<
                (TracerProvider, opentelemetry_sdk::trace::Tracer),
                TracerInitializationError,
            > + Send
            + 'static,
        SetSubscriber: Fn(
                Layered<OpenTelemetryLayer<Registry, opentelemetry_sdk::trace::Tracer>, Registry>,
            ) -> TracerInitializationResult<()>
            + Send
            + 'static,
    {
        let (set_subscriber_result_tx, set_subscriber_result_rx) = tokio::sync::oneshot::channel();
        let shutdown_notify = self.shutdown_notify.clone();
        self.runtime.spawn(async move {
            match init_tracer_provider() {
                Ok((provider, tracer)) => {
                    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

                    // Use the tracing subscriber `Registry`, or any other subscriber
                    // that impls `LookupSpan`
                    let subscriber = Registry::default().with(telemetry);
                    let result = set_subscriber(subscriber);

                    match set_subscriber_result_tx.send(result) {
                        Ok(_) => {
                            // wait for shutdown notification
                            shutdown_notify.notified().await;
                            provider.force_flush();
                            // notify the shutdown is complete
                            shutdown_notify.notify_one();
                        }
                        Err(result) => {
                            // In this case, the receiver never receives the "ready" signal, so
                            // the `start_tracer` function will timeout and return an errors, so we do
                            // not need to wait for a shutdown and force flush.
                            eprintln!(
                                "failed to send export initialization signal: {}",
                                result.map_or_else(|e| e.to_string(), |_| "()".to_string())
                            );
                        }
                    }
                }
                Err(e) => {
                    if let Err(result) = set_subscriber_result_tx.send(Err(e)) {
                        eprintln!(
                            "failed to send export initialization signal: {}",
                            result.map_or_else(|e| e.to_string(), |_| "()".to_string())
                        );
                    }
                }
            }
        });

        // We should not be in an existing tokio runtime, so we create a new one
        // and block on the result of the `set_subscriber` function. This ensures
        // the function does not return until the subscriber is set and we are ready
        // to start collecting trace data.
        let wait_for_startup_runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(TracerInitializationError::RuntimeInitialization)?;
        let _guard = wait_for_startup_runtime.enter();
        wait_for_startup_runtime
            .block_on(tokio::time::timeout(to, set_subscriber_result_rx))
            .map_err(TracerInitializationError::from)
            .and_then(|r| {
                r.map_err(TracerInitializationError::from)
                    .and_then(|r| r.map_err(TracerInitializationError::from))
            })
    }

    pub(crate) async fn shutdown(&self) {
        // notify the background process to shutdown
        self.shutdown_notify.notify_one();
        // wait to be notified that the shutdown is complete
        self.shutdown_notify.notified().await;
    }
}

pub(super) type TracerInitializationResult<T> = Result<T, TracerInitializationError>;

fn init_runtime() -> TracerInitializationResult<Runtime> {
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(TracerInitializationError::RuntimeInitialization)
}

#[cfg(test)]
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
