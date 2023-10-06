use std::path::Path;

use handlebars::{RenderError, TemplateError};

#[derive(serde::Serialize, Default)]
struct Data {
    host_package: String,
    tracing_subscriber_module_name: String,
    version: String,
}

impl Data {
    fn new(host_package: String, tracing_subscriber_module_name: String) -> Self {
        Self {
            host_package,
            tracing_subscriber_module_name,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed open file for writing: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to render template: {0}")]
    Render(#[from] RenderError),
    #[error("failed to initialize template: {0}")]
    Template(#[from] Box<TemplateError>),
}

macro_rules! include_stub_and_init {
    ($directory: ident, $template_name: tt, $hb: ident) => {
        std::fs::create_dir_all($directory.join($template_name)).map_err(Error::from)?;
        $hb.register_template_string(
            concat!($template_name, "__init__.py"),
            include_str!(concat!(
                "../assets/python_stubs/",
                $template_name,
                "__init__.py"
            )),
        )
        .map_err(Box::new)
        .map_err(Error::from)?;
        $hb.register_template_string(
            concat!($template_name, "__init__.pyi"),
            include_str!(concat!(
                "../assets/python_stubs/",
                $template_name,
                "__init__.pyi"
            )),
        )
        .map_err(Box::new)
        .map_err(Error::from)?;
    };
}

///
/// # Errors
///
/// Will return an error if the stub files cannot be written to the given directory.
pub fn write_stub_files(
    host_package: &str,
    tracing_subscriber_module_name: &str,
    directory: &Path,
) -> Result<(), Error> {
    let mut hb = handlebars::Handlebars::new();
    include_stub_and_init!(directory, "", hb);
    include_stub_and_init!(directory, "subscriber/", hb);
    include_stub_and_init!(directory, "layers/", hb);
    include_stub_and_init!(directory, "layers/otel_file/", hb);
    include_stub_and_init!(directory, "layers/otel_otlp/", hb);
    let data = Data::new(
        host_package.to_string(),
        tracing_subscriber_module_name.to_string(),
    );
    for name in hb.get_templates().keys() {
        let writer = std::fs::File::create(directory.join(name)).map_err(Error::from)?;
        hb.render_to_write(name, &data, writer)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    #[test]
    fn test_build_stub_files() {
        super::write_stub_files(
            "example",
            "_tracing_subscriber",
            std::path::Path::new("target/stubs"),
        )
        .unwrap();
    }
}
