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

//! This module provides utilities for exporting spans to various backends
//! from Rust.

pub(super) mod common;
mod contextmanager;
mod export_process;
pub(crate) mod layer;
pub(crate) mod subscriber;

// / Adds the pyo3-opentelemetry export module to your parent module. The upshot here
// / is that the Python package will contain `{name}.export.{stdout/otlp/py_otlp}`,
// / each with an async context manager that can be used on the Python side to
// / export spans.
// /
// / # Arguments
// / * `name` - The name of the parent module.
// / * `py` - The Python interpreter.
// / * `m` - The parent module.
// /
// / # Returns
// / * `PyResult<()>` - The result of adding the submodule to the parent module.
// /
// / # Errors
// / * If the submodule cannot be added to the parent module.
// /
// pub(crate) fn init_submodule(name: &str, py: Python, m: &PyModule) -> PyResult<()> {
//     let modules = py.import("sys")?.getattr("modules")?;
//
//     #[cfg(feature = "export-stdout")]
//     {
//         let submod = pyo3::types::PyModule::new(py, "stdout")?;
//         stdout::init_submodule("stdout", py, submod)?;
//         modules.set_item(format!("{name}.export.stdout"), submod)?;
//         m.add_submodule(submod)?;
//     }
//     #[cfg(feature = "export-otlp")]
//     {
//         let submod = pyo3::types::PyModule::new(py, "otlp")?;
//         otlp::init_submodule("otlp", py, submod)?;
//         modules.set_item(format!("{name}.export.otlp"), submod)?;
//         m.add_submodule(submod)?;
//     }
//     #[cfg(feature = "export-py-otlp")]
//     {
//         let submod = pyo3::types::PyModule::new(py, "py_otlp")?;
//         py_otlp::init_submodule("py_otlp", py, submod)?;
//         modules.set_item(format!("{name}.export.py_otlp"), submod)?;
//         m.add_submodule(submod)?;
//     }
//
//     Ok(())
// }
