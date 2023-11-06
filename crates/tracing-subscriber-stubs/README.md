# PyO3 OpenTelemetry

## Background

### What this is

pyo3_opentelemetry_stubs provides a function for adding stub files for downstream `pyo3` extension modules that use the `pyo3-tracing-subscriber` library. 

### What this is not

* This does not produce any Python source code (exception `__init__.py` for conveniencing import) or any Rust code that will serve as a `pyo3` extension module. Use the `pyo3-tracing-subscriber` crate to add the tracing subscriber submodule to your extension module.

## Usage

> For a functioning example, see `examples/pyo3-opentelemetry-lib/build.rs` at the root of this repository.

Given a `pyo3` extension module named "my_module" that uses the `pyo3-tracing-subscriber` crate to expose tracing subscriber configuration and context manager classes from "my_module._tracing_subscriber", in the upstream `build.rs` file:

```rs
use pyo3_tracing_subscriber_stubs::write_stub_files;

fn main() {
    let target_dir = std::path::Path::new("./my_module/_tracing_subscriber");
    std::fs::remove_dir_all(target_dir).unwrap();
    write_stub_files(
        "my_module",
        "_tracing_subscriber",
        target_dir,
        true, // layer_otel_otlp_file feature enabled
        true, // layer_otel_otlp feature enabled
    )
    .unwrap();
}
```



