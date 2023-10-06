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

// Covers correctness, suspicious, style, complexity, and perf
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::cargo)]
#![warn(clippy::nursery)]
// Has false positives that conflict with unreachable_pub
#![allow(clippy::redundant_pub_crate)]
#![deny(
    absolute_paths_not_starting_with_crate,
    anonymous_parameters,
    bad_style,
    dead_code,
    keyword_idents,
    improper_ctypes,
    macro_use_extern_crate,
    meta_variable_misuse, // May have false positives
    missing_abi,
    missing_debug_implementations, // can affect compile time/code size
    missing_docs,
    no_mangle_generic_items,
    non_shorthand_field_patterns,
    noop_method_call,
    overflowing_literals,
    path_statements,
    patterns_in_fns_without_body,
    pointer_structural_match,
    private_in_public,
    semicolon_in_expressions_from_macros,
    trivial_casts,
    trivial_numeric_casts,
    unconditional_recursion,
    unreachable_pub,
    unsafe_code,
    unused,
    unused_allocation,
    unused_comparisons,
    unused_extern_crates,
    unused_import_braces,
    unused_lifetimes,
    unused_parens,
    variant_size_differences,
    while_true
)]
//! This crate provides utilities for configuring and initializing a tracing subscriber from
//! Python. Because Rust pyo3-based Python packages are binaries, these utilities are exposed
//! as a [`pyo3::PyModule`] which can then be added to upstream pyo3 libraries.
//!
//! # Features
//!
//! * `layer-otel-file` - exports trace data with [opentelemetry-stdout](https://lib.rs/crates/opentelemetry-stdout). See [`crate::layers::otel_file`].
//! * `layer-otel-otlp` - exports trace data with [opentelemetry-otlp](https://lib.rs/crates/opentelemetry-otlp). See [`crate::layers::otel_otlp`].
//!
//! # Requirements and Limitations
//!
//! * The tracing subscribers initialized and configured _only_ capture tracing data for the pyo3
//! library which adds the  pyo3-tracing-subscriber` module. Separate crates require separate
//! bootstrapping.
//! * Python users can initialize tracing subscribers using context managers either globally, in
//! which case they can only initialize once, or per-thread which is incompatible with Python
//! `async/await`.
//! * The OTel OTLP layer requires a heuristic based timeout upon context manager exit to ensure
//! trace data on the Rust side is flushed to the OTLP collector. This issue persists despite calls
//! to [`opentelemetry_sdk::trace::provider::TracerProvider::force_flush`] and [`opentelemetry::global::shutdown_tracer_provider`].
//!
//! # Related Crates
//!
//! * [`pyo3-opentelemetry`](https://crates.io/crates/pyo3-opentelemetry) - propagates
//! OpenTelemetry contexts from Python into Rust.
//! * [pyo3-tracing-subscriber-stubs](https://crates.io/crates/pyo3-tracing-subscriber-stubs) -
//! evaluates Python stub templates for use in upstream pyo3 library build scripts.
//!
//! # Examples
//!
//! ```
//! use pyo3_tracing_subscriber::pypropagate;
//! use pyo3::prelude::*;
//! use tracing::instrument;
//!
//! const MY_PACKAGE_NAME: &str = "example";
//! const TRACING_SUBSCRIBER_SUBPACKAGE_NAME: &str = "tracing_subscriber";
//!
//! #[pymodule]
//! fn example(_py: Python, m: &PyModule) -> PyResult<()> {
//!     // add your functions, modules, and classes
//!     let tracing_subscriber = PyModule::new(py, TRACING_SUBSCRIBER_SUBPACKAGE_NAME)?;
//!     pyo3_tracing_subscriber::add_submodule(
//!         format!("{MY_PACKAGE_NAME}.{TRACING_SUBSCRIBER_SUBPACKAGE_NAME}"),
//!         py,
//!         tracing_subscriber,
//!     )?;
//!     m.add_submodule(tracing_subscriber)?;
//!     Ok(())
//! }
//! ```
//!
//! Then in Python:
//!
//! ```python
//! import asyncio
//! from example.tracing_subscriber import Tracing
//!
//!
//! async main():
//!     async with Tracing():
//!         # do stuff
//!         pass
//!
//!
//! if __name__ == "__main__":
//!     asyncio.run(main())
//! ```
use pyo3::{types::PyModule, PyResult, Python};
use rigetti_pyo3::create_init_submodule;

use self::{
    contextmanager::{CurrentThreadTracingConfig, GlobalTracingConfig, TracingContextManagerError},
    export_process::{BatchConfig, SimpleConfig, TracingShutdownError, TracingStartError},
};
pub use contextmanager::Tracing;

mod contextmanager;
mod export_process;
pub(crate) mod layers;
pub(crate) mod subscriber;

create_init_submodule! {
    classes: [
        Tracing,
        GlobalTracingConfig,
        CurrentThreadTracingConfig,
        BatchConfig,
        SimpleConfig
    ],
    errors: [TracingContextManagerError, TracingStartError, TracingShutdownError],
    submodules: [
        "layers": layers::init_submodule,
        "subscriber": subscriber::init_submodule
    ],
}

/// Add the tracing submodule to the given module. This will add the submodule to the `sys.modules`
/// dictionary so that it can be imported from Python. This function will add the folloiwng:
///
/// * [`Tracing`] - a Python context manager which initializes the configured tracing subscriber.
/// * [`GlobalTracingConfig`] - a Python context manager which sets the configured tracing subscriber
/// as the global default (ie `tracing::subscriber::set_global_default`). The `Tracing` context
/// manager can be used _only once_ with this configuration per process.
/// * [`CurrentThreadTracingConfig`] - a Python context manager which sets the configured tracing
/// subscriber as the current thread default (ie `tracing::subscriber::set_default`). As the
/// context manager exits, the guard is dropped and the tracing subscriber can be re-initialized
/// with another default. Note, the default tracing subscriber will _not_ capture threads across
/// `async/await` boundaries that call `pyo3_asyncio::tokio::future_into_py`.
/// * [`BatchConfig`] - a Python context manager which configures the tracing subscriber to export
/// trace data in batch. As the `Tracing` context manager enters, a Tokio runtime is initialized
/// and will run in the background until the context manager exits.
/// * [`SimpleConfig`] - a Python context manager which configures the tracing subscriber to export
/// trace data in a non-batch manner. This only initializes a Tokio runtime if the underlying layer
/// requires an asynchronous runtime to export trace data (ie the `opentelemetry-otlp` layer).
/// * [`layers`] - a submodule which contains different layers to add to the tracing subscriber.
/// Currently supported:
///     * `opentelemetry-stdout` - a layer which exports trace data to stdout (requires the `layer-otel-file` feature).
///     * `opentelemetry-otlp` - a layer which exports trace data to an OpenTelemetry collector (requires the `layer-otel-otlp` feature).
/// * [`subscriber`] - a submodule which contains utilities for initialing the tracing subscriber
/// with the configured layer. Currently, the tracing subscriber is initialized as
/// `tracing::subscriber::Registry::default().with(layer)`.
///
/// Additionally, the following exceptions are added to the submodule:
///
/// * [`TracingContextManagerError`] - raised when the `Tracing` context manager's methods are not
/// invoked in the correct order or multiplicity.
/// * [`TracingStartError`] - raised if the user specified tracing layer or subscriber fails to build
/// and initialize properly on context manager entry.
/// * [`TracingShutdownError`] - raised if the tracing layer or subscriber fails to shutdown properly on context manager exit.
///
/// For detailed documentation on usage from the Python side, see the
/// `pyo3-tracing-subscriber-stubs` crate.
///
/// # Arguments
///
/// * `name` - the fully qualified name of the submodule tracing subscriber module. For instance,
/// if your package is named `my_package` and you want to add the tracing subscriber submodule
/// `tracing_subscriber`, then `name` should be `my_package.tracing_subscriber`.
/// * `py` - the Python GIL token.
/// * `m` - the parent module to which the tracing subscriber submodule should be added.
///
/// # Errors
///
/// * `PyErr` if the submodule cannot be added.
pub fn add_submodule(name: &str, py: Python, m: &PyModule) -> PyResult<()> {
    init_submodule(name, py, m)?;
    let modules = py.import("sys")?.getattr("modules")?;
    modules.set_item(name, m)?;
    Ok(())
}

#[cfg(all(not(feature = "layer-otel-file"), not(feature = "layer-otel-otlp")))]
fn unsupported_default_initialization<T>(value: Option<T>) -> PyResult<T> {
    value.ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(
            "this package does not support default file or otlp layers",
        )
    })
}
