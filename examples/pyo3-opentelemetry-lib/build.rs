use pyo3_tracing_subscriber_stubs::write_stub_files;

fn main() {
    write_stub_files(
        "pyo3_opentelemetry_lib",
        "_tracing_subscriber",
        std::path::Path::new("./pyo3_opentelemetry_lib/_tracing_subscriber"),
        true,
        true,
    )
    .unwrap();
}
