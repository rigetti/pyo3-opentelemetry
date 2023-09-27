use std::time::Duration;

use once_cell::sync::OnceCell;
use pyo3::prelude::*;

use opentelemetry_sdk::trace::TracerProvider;
use rigetti_pyo3::{py_wrap_error, ToPythonError};

use self::trace::BatchExportProcess;

pub(super) mod trace;
pub(super) mod wg;

static BATCH_EXPORT_PROCESS: OnceCell<BatchExportProcess> = OnceCell::new();

py_wrap_error!(
    export,
    trace::TracerInitializationError,
    Pyo3OpentelemetryError,
    rigetti_pyo3::pyo3::exceptions::PyRuntimeError
);

/// Notifies the background batch export process that it can shutdown and waits
/// for that background process to complete any clean up tasks, such a force
/// flushing the global tracer provider. This may only be called once per
/// process and only after `start_tracer` has been called.
pub(super) fn stop(py: Python<'_>) -> PyResult<&PyAny> {
    let batch_export_process = BATCH_EXPORT_PROCESS
        .get()
        .ok_or(trace::TracerInitializationError::Uninitialized)
        .map_err(trace::TracerInitializationError::to_py_err)?;

    pyo3_asyncio::tokio::future_into_py(py, async {
        batch_export_process.shutdown().await;
        Ok(())
    })
}

/// Initializes the tracer and tracer provider, sets the tracing subscriber global
/// default, and initializes a background runtime for batch exporting spans. This
/// can only be called once per process.
///
/// Note, this cannot be called within an existing `tokio::Runtime` (ie no nested
/// runtimes).
pub(super) fn start_tracer<F>(f: F, timeout: Duration) -> trace::TracerInitializationResult<()>
where
    F: FnOnce() -> Result<
            (TracerProvider, opentelemetry_sdk::trace::Tracer),
            trace::TracerInitializationError,
        > + Send
        + 'static,
{
    if BATCH_EXPORT_PROCESS.get().is_some() {
        return Err(trace::TracerInitializationError::AlreadyInitialized);
    }
    BATCH_EXPORT_PROCESS.get_or_try_init(|| -> trace::TracerInitializationResult<_> {
        let batch_export_process = BatchExportProcess::new()?;

        batch_export_process.start_tracer(
            f,
            |subscriber| {
                tracing::subscriber::set_global_default(subscriber)
                    .map_err(trace::TracerInitializationError::from)
            },
            timeout,
        )?;
        Ok(batch_export_process)
    })?;
    Ok(())
}
