# PyO3 Tracing Crates

This repository contains four crates to support the ability of upstream `pyo3` extension modules to support the ability of their dependents to gather tracing telemetry from the instrumented Rust source code:

* [crates/opentelemetry](./crates/opentelemetry): propagates OpenTelemetry context from Python into Rust.
* [crates/opentelemetry-macros](./crates/opentelemetry-macros): defines proc macros for `pyo3-opentelemetry`.
* [crates/tracing-subscriber](./crates/tracing-subscriber): supports configuration and initialization of Rust tracing subscribers from Python.

For a functional example of usage of all of these crates, see [examples/pyo3-opentelemetry-lib](./examples/pyo3-opentelemetry-lib).

## Development

### Rust 

It should be sufficient to [install the Rust toolchain](https://rustup.rs/) and [cargo-make](https://github.com/sagiegurari/cargo-make). Then:

```shell
cargo make check-all
```

### Python

Install:

* Python - installation through [pyenv](https://github.com/pyenv/pyenv) is recommended. 
* [Poetry](https://python-poetry.org/docs/#installation) - for installing plain Python dependencies.

