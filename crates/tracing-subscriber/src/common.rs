use std::borrow::Cow;
use std::collections::HashMap;

use opentelemetry::InstrumentationScope;
/// This module contains a number of `rigetti-pyo3` ports which were
/// backed out due to build issues involving the `pyo3/extension-module`
/// feature. This should be replaced upon resolution of
/// <https://github.com/rigetti/pyo3-opentelemetry/issues/15/>.
use pyo3::prelude::*;
use pyo3::PyErr;

/// A macro for initializing a submodule.
#[macro_export]
macro_rules! create_init_submodule {
    (
        $(classes: [ $($class: ty),+ ],)?
        $(consts: [ $($const: ident),+ ],)?
        $(errors: [ $($error: ty),+ ],)?
        $(funcs: [ $($func: path),+ ],)?
        $(submodules: [ $($mod_name: literal: $init_submod: path),+ ],)?
    ) => {
        pub(crate) fn init_submodule<'py>(_name: &str, _py: pyo3::Python<'py>, m: &pyo3::Bound<'py, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
            $($(
            m.add_class::<$class>()?;
            )+)?
            $($(
            m.add(::std::stringify!($const), $crate::ToPython::<pyo3::Py<pyo3::PyAny>>::to_python(&$const, _py)?)?;
            )+)?
            $($(
            m.add(std::stringify!($error), _py.get_type::<$error>())?;
            )+)?
            $($(
            m.add_function(pyo3::wrap_pyfunction!($func, m)?)?;
            )+)?
            $(
                let modules = _py.import("sys")?.getattr("modules")?;
                $(
                let submod = pyo3::types::PyModule::new(_py, $mod_name)?;
                let qualified_name = format!("{}.{}", _name, $mod_name);
                $init_submod(&qualified_name, _py, &submod)?;
                m.add_submodule(&submod)?;
                modules.set_item(&qualified_name, submod)?;
                )+
            )?
            Ok(())
        }
    }
}

/// A macro for wrapping a Rust error as a type error. Implements [`ToPythonError`].
#[macro_export]
macro_rules! py_wrap_error {
    ($module: ident, $rust: ty, $python: ident, $base: ty) => {
        pyo3::create_exception!($module, $python, $base);

        impl $crate::common::ToPythonError for $rust {
            fn to_py_err(self) -> pyo3::PyErr {
                <$python>::new_err(self.to_string())
            }
        }
    };
}

/// A macro for wrapping a Rust error.
#[macro_export]
macro_rules! wrap_error {
    ($name: ident ($inner: ty)$(;)?) => {
        #[derive(Debug)]
        #[repr(transparent)]
        pub(crate) struct $name($inner);

        impl From<$inner> for $name {
            fn from(inner: $inner) -> Self {
                Self(inner)
            }
        }

        impl From<$name> for $inner {
            fn from(outer: $name) -> Self {
                outer.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl ::std::error::Error for $name {}
    };
}

/// Converts to a Python error.
pub(crate) trait ToPythonError {
    /// Convert this error into a [`PyErr`](crate::pyo3::PyErr).
    fn to_py_err(self) -> PyErr;
}

impl ToPythonError for PyErr {
    fn to_py_err(self) -> PyErr {
        self
    }
}

impl ToPythonError for std::convert::Infallible {
    fn to_py_err(self) -> PyErr {
        unreachable!("Infallible can never happen")
    }
}

#[pyclass(name = "InstrumentationLibrary")]
#[derive(Debug, Clone)]
pub(crate) struct PyInstrumentationLibrary {
    name: String,
    version: Option<String>,
    schema_url: Option<String>,
    attributes: HashMap<String, String>,
}

#[pymethods]
impl PyInstrumentationLibrary {
    #[new]
    #[pyo3(signature = (name, /, version=None, schema_url=None, attributes=None))]
    fn new(
        name: String,
        version: Option<String>,
        schema_url: Option<String>,
        attributes: Option<HashMap<String, String>>,
    ) -> Self {
        let attributes = attributes.unwrap_or_default();
        Self {
            name,
            version,
            schema_url,
            attributes,
        }
    }
}

impl From<PyInstrumentationLibrary> for InstrumentationScope {
    fn from(py_instrumentation_library: PyInstrumentationLibrary) -> Self {
        let mut builder = Self::builder(Cow::from(py_instrumentation_library.name));
        if let Some(version) = py_instrumentation_library.version {
            builder = builder.with_version(Cow::from(version));
        }
        if let Some(schema_url) = py_instrumentation_library.schema_url {
            builder = builder.with_schema_url(Cow::from(schema_url));
        }
        let mut attributes = Vec::new();
        for (key, value) in py_instrumentation_library.attributes {
            let kv = opentelemetry::KeyValue::new(
                opentelemetry::Key::new(key),
                opentelemetry::Value::from(value),
            );
            attributes.push(kv);
        }
        builder = builder.with_attributes(attributes);

        builder.build()
    }
}

create_init_submodule! {
    classes: [ PyInstrumentationLibrary ],
}
