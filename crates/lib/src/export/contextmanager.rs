use pyo3::{exceptions::PyRuntimeError, prelude::*};

use rigetti_pyo3::{py_wrap_error, wrap_error, ToPythonError};

use super::{
    export_process::{
        ExportProcess, ExportProcessConfig, RustTracingConfigError, RustTracingShutdownError,
        RustTracingStartError,
    },
    subscriber::PyConfig,
};

#[pyclass]
#[derive(Clone)]
struct GlobalTracingConfig {
    pub(crate) export_process: ExportProcessConfig,
}

#[pyclass]
#[derive(Clone)]
struct CurrentThreadTracingConfig {
    pub(crate) subscriber: PyConfig,
}

#[derive(FromPyObject)]
pub(crate) enum TracingConfig {
    Global(GlobalTracingConfig),
    CurrentThread(CurrentThreadTracingConfig),
}

#[pyclass]
pub(super) struct Tracing {
    export_process: Option<ExportProcess>,
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
    #[pyo3(signature = (config))]
    pub fn new(config: TracingConfig) -> PyResult<Self> {
        let export_process = Some(
            config
                .try_into()
                .map_err(RustTracingConfigError::from)
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
    ) -> PyResult<&'a PyAny> {
        let export_process = self
            .export_process
            .take()
            .ok_or(ContextManagerError::Exit)
            .map_err(RustContextManagerError::from)
            .map_err(ToPythonError::to_py_err)?;
        pyo3_asyncio::tokio::future_into_py(py, async move {
            export_process
                .shutdown()
                .await
                .map_err(RustTracingShutdownError::from)
                .map_err(ToPythonError::to_py_err)
        })
    }
}
