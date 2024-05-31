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

use crate::create_init_submodule;
use opentelemetry_sdk::runtime::TokioCurrentThread;
use pyo3::prelude::*;
use tracing_subscriber::Layer;

use super::{build_env_filter, force_flush_provider_as_shutdown, LayerBuildResult, WithShutdown};

/// Configures the [`opentelemetry-stdout`] crate layer. If [`file_path`] is None, the layer
/// will write to stdout.
#[pyclass]
#[derive(Clone, Debug, Default)]
pub(crate) struct Config {
    pub(crate) file_path: Option<String>,
    pub(crate) filter: Option<String>,
}

#[pymethods]
impl Config {
    #[new]
    #[pyo3(signature = (/, file_path = None, filter = None))]
    const fn new(file_path: Option<String>, filter: Option<String>) -> Self {
        Self { file_path, filter }
    }
}

impl crate::layers::Config for Config {
    fn requires_runtime(&self) -> bool {
        false
    }

    fn build(&self, batch: bool) -> LayerBuildResult<WithShutdown> {
        use opentelemetry::trace::TracerProvider as _;
        let exporter_builder = opentelemetry_stdout::SpanExporter::builder();
        let exporter_builder = match self.file_path.as_ref() {
            Some(file_path) => {
                let file = std::fs::File::create(file_path).map_err(BuildError::from)?;
                exporter_builder.with_writer(file)
            }
            None => exporter_builder,
        };
        let provider = if batch {
            opentelemetry_sdk::trace::TracerProvider::builder()
                .with_batch_exporter(exporter_builder.build(), TokioCurrentThread)
                .build()
        } else {
            opentelemetry_sdk::trace::TracerProvider::builder()
                .with_simple_exporter(exporter_builder.build())
                .build()
        };
        let tracer = provider.tracer("pyo3-opentelemetry-stdout");
        let env_filter = build_env_filter(self.filter.clone())?;
        let layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(env_filter);
        Ok(WithShutdown {
            layer: Box::new(layer),
            shutdown: force_flush_provider_as_shutdown(provider, None),
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum BuildError {
    #[error("failed to initialize file span exporter for specified file path: {0}")]
    InvalidFile(#[from] std::io::Error),
}

create_init_submodule! {
    classes: [ Config ],
}
