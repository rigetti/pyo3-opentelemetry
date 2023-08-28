# PyO3 OpenTelemetry

## Background

### What this is

pyo3_opentelemetry provides a macro to simply and easily instrument your PyO3 bindings so that OpenTelemetry contexts can be easily passed from a Python caller into a Rust library. The `#[pypropagate]` macro instruments your Rust functions for you so that the global Python OpenTelemetry context is shared across the FFI boundary.

### What this is not

* This (currently) does not support propagating an OpenTelemetry context from Rust into Python.
* This does not "magically" instrument Rust code. Without the `#[pypropagate]` attribute, Rust code is unaffected and will not attach the Python OpenTelemetry context.
* This does not facilitate the processing or collection of OpenTelemetry spans; you still need to initialize and flush  tracing providers and subscribers separately in Python and Rust. For more information, please see the respective OpenTelemetry documentation for [Python](https://opentelemetry.io/docs/instrumentation/python/) and [Rust](https://opentelemetry.io/docs/instrumentation/rust/).


### What this is

This repo contains utilities for automatically passing OpenTelemetry contexts from Python into Rust.

### What this is not

* This does not facilitate the processing of spans. You still need to separately instrument OpenTelemetry processors on both the Rust and Python side.
* This does not allow you to propagate context into _any_ Rust code from Python. It requires instrumentation of the underlying Rust source code.
* While this repository could extend to pass OpenTelemetry contexts _from Rust into Python_, it currently does not.

## Usage

From Rust:

```rs
use pyo3_opentelemetry::prelude::*;
use pyo3::prelude::*;
use tracing::instrument;

#[pypropagate]
#[pyfunction]
#[instrument]
fn my_function() {
  println!("span \"my_function\" is active and will share the Python OpenTelemetry context");
}

#[pymodule]
fn my_module(_py: Python, m: &PyModule) -> PyResult<()> {
   m.add_function(wrap_pyfunction!(my_function, m)?)?;
   Ok(())
}
```

These features require no Python code changes, however, [opentelemetry-api](https://pypi.org/project/opentelemetry-api/) must be installed.

## Development

| Directory | Purpose |
|-----------|---------|
| crates/macros | Rust macro definitions |
| crates/lib | Supporting Rust functions that get OTel context from Python. |
| examples/pyo3-opentelemetry-lib | maturin PyO3 project with Python test assertions on Rust OTel spans |

### Rust 

It should be sufficient to [install the Rust toolchain](https://rustup.rs/) and [cargo-make](https://github.com/sagiegurari/cargo-make). Then:

```sh
cargo make check-all
```

### Python

Install:

* Python - installation through [pyenv](https://github.com/pyenv/pyenv) is recommended. 
* [Poetry](https://python-poetry.org/docs/#installation) - for installing plain Python dependencies.

#### Python Example

[examples/pyo3-opentelemetry-lib](./examples/pyo3-opentelemetry-lib/) contains a full end-to-end pyo3 project with pytests. To build and run them:

```sh
cd examples/pyo3-opentelemetry-lib
cargo make python-check-all 
```

