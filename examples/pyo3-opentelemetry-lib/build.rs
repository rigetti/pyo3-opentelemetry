use pyo3_tracing_subscriber::stubs::write_stub_files;

fn main() {
    let target_dir = std::path::Path::new("./pyo3_opentelemetry_lib/_tracing_subscriber");
    std::fs::remove_dir_all(target_dir).unwrap();
    write_stub_files(
        "pyo3_opentelemetry_lib",
        "_tracing_subscriber",
        target_dir,
        true,
        true,
    )
    .unwrap();
}
