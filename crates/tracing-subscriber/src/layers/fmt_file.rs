use pyo3::prelude::*;
use rigetti_pyo3::create_init_submodule;
use tracing_subscriber::Layer;

use super::{build_env_filter, LayerBuildResult, ShutdownResult, WithShutdown};

/// Configures the [`opentelemetry-stdout`] crate layer. If [`file_path`] is None, the layer
/// will write to stdout.
#[pyclass]
#[derive(Clone, Debug)]
pub(crate) struct Config {
    pub(crate) file_path: Option<String>,
    pub(crate) pretty: bool,
    pub(crate) filter: Option<String>,
    pub(crate) json: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            file_path: None,
            pretty: false,
            filter: None,
            json: true,
        }
    }
}

#[pymethods]
impl Config {
    #[new]
    #[pyo3(signature = (/, file_path = None, pretty = false, filter = None, json = true))]
    const fn new(
        file_path: Option<String>,
        pretty: bool,
        filter: Option<String>,
        json: bool,
    ) -> Self {
        Self {
            file_path,
            pretty,
            filter,
            json,
        }
    }
}

impl crate::layers::Config for Config {
    fn requires_runtime(&self) -> bool {
        false
    }

    fn build(&self, _batch: bool) -> LayerBuildResult<WithShutdown> {
        let filter = build_env_filter(self.filter.clone())?;
        let layer = if let Some(file_path) = self.file_path.as_ref() {
            let file = std::fs::File::create(file_path).map_err(BuildError::from)?;
            if self.json && self.pretty {
                tracing_subscriber::fmt::layer()
                    .json()
                    .pretty()
                    .with_writer(file)
                    .with_filter(filter)
                    .boxed()
            } else if self.json {
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_writer(file)
                    .with_filter(filter)
                    .boxed()
            } else if self.pretty {
                tracing_subscriber::fmt::layer()
                    .pretty()
                    .with_writer(file)
                    .with_filter(filter)
                    .boxed()
            } else {
                tracing_subscriber::fmt::layer().with_writer(file).boxed()
            }
        } else if self.json && self.pretty {
            tracing_subscriber::fmt::layer()
                .json()
                .pretty()
                .with_filter(filter)
                .boxed()
        } else if self.json {
            tracing_subscriber::fmt::layer()
                .json()
                .with_filter(filter)
                .boxed()
        } else if self.pretty {
            tracing_subscriber::fmt::layer()
                .pretty()
                .with_filter(filter)
                .boxed()
        } else {
            tracing_subscriber::fmt::layer().with_filter(filter).boxed()
        };

        Ok(WithShutdown {
            layer: Box::new(layer),
            shutdown:     Box::new(
                move || -> std::pin::Pin<Box<dyn std::future::Future<Output = ShutdownResult<()>> + Send + Sync>> {
                    Box::pin(async move {
                        Ok(())
                    })
                },
            )
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum BuildError {
    #[error("failed to initialize stdout layer for specified file path: {0}")]
    InvalidFile(#[from] std::io::Error),
}

create_init_submodule! {
    classes: [ Config ],
}
