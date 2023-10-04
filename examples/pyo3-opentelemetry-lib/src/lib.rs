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
    // unused_qualifications,
    variant_size_differences,
    while_true
)]
//! This crate demonstrates example usage of the `pypropagate` macro. It defines example functions and
//! methods, which may wrap Rust async functions, and be called from Python. The generated Python
//! bindings are used in the Poetry package within this crate to assert that contexts are properly
//! set and propagated across the Python to Rust boundary.
use std::collections::HashMap;

use opentelemetry::propagation::TextMapPropagator;
use opentelemetry::trace::FutureExt;
use opentelemetry_api::trace::TraceContextExt;
use opentelemetry_api::Context;
use pyo3::prelude::*;
use pyo3_opentelemetry::pypropagate;
use tracing::instrument;

#[instrument]
fn example_function_impl() -> HashMap<String, String> {
    let span = tracing::info_span!("example_function");
    let _guard = span.enter();

    // FIXME
    println!(
        "example_function_impl> {}",
        Context::current().span().span_context().trace_id()
    );

    let propagator = opentelemetry_sdk::propagation::TraceContextPropagator::new();
    let mut injector = HashMap::new();
    propagator.inject(&mut injector);
    injector
}

#[instrument]
async fn example_function_impl_async() -> PyResult<HashMap<String, String>> {
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    Ok(example_function_impl())
}

/// An example function that will call a function containing and span and returns a
/// HashMap with the propagated OTel context.
#[pypropagate]
#[pyfunction]
pub fn example_function(py: Python<'_>) -> HashMap<String, String> {
    // println!(
    //     "Hello from example_function {}",
    //     Context::current().span().span_context().trace_id()
    // );
    // FIXME
    println!(
        "example_function> is recording?? {}",
        Context::current().span().is_recording()
    );
    example_function_impl()
}

/// An example async function that will call a function containing and span and returns a
/// HashMap with the propagated OTel context.
#[pypropagate]
#[pyfunction]
pub fn example_function_async<'a>(py: Python<'a>) -> PyResult<&'a PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, example_function_impl_async().with_current_context())
}

/// An example pyclass sturct that will have methods that propagate their OTel contexts from
/// the calling Python context.
#[pyclass]
#[derive(Debug)]
pub struct ExampleStruct;

#[pypropagate]
#[pymethods]
impl ExampleStruct {
    #[new]
    fn new(py: Python<'_>) -> Self {
        Self
    }

    /// An example struct method that will call a function containing and span and returns a
    /// HashMap with the propagated OTel context.
    pub fn example_method(&self, py: Python<'_>) -> HashMap<String, String> {
        example_function_impl()
    }

    /// An example async struct method that will call a function containing and span and returns a
    /// HashMap with the propagated OTel context.
    pub fn example_method_async<'a>(&self, py: Python<'a>) -> PyResult<&'a PyAny> {
        pyo3_asyncio::tokio::future_into_py(
            py,
            example_function_impl_async().with_current_context(),
        )
    }
}

#[pymodule]
fn pyo3_opentelemetry_lib(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<ExampleStruct>()?;
    m.add_function(wrap_pyfunction!(example_function, m)?)?;
    m.add_function(wrap_pyfunction!(example_function_async, m)?)?;

    let tracing_subscriber = PyModule::new(py, "_tracing_subscriber")?;
    pyo3_opentelemetry::tracing_subscriber::add_submodule(
        "pyo3_opentelemetry_lib._tracing_subscriber",
        py,
        tracing_subscriber,
    )?;
    m.add_submodule(tracing_subscriber)?;
    Ok(())
}
