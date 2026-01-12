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

use std::{future::Future, io::Write, sync::Arc};

use crate::create_init_submodule;
use opentelemetry_proto::transform::{
    common::tonic::ResourceAttributesWithSchema, trace::tonic::group_spans_by_resource_and_scope,
};
use opentelemetry_sdk::error::{OTelSdkError, OTelSdkResult};
use pyo3::prelude::*;

use super::{build_env_filter, force_flush_provider_as_shutdown, LayerBuildResult, WithShutdown};
use crate::common::PyInstrumentationLibrary;
use tracing_subscriber::Layer;

/// Configures the [`opentelemetry-stdout`] crate layer. If [`file_path`] is None, the layer
/// will write to stdout.
#[pyclass]
#[derive(Clone, Debug, Default)]
pub(crate) struct Config {
    pub(crate) file_path: Option<String>,
    pub(crate) filter: Option<String>,
    pub(crate) instrumentation_library: Option<PyInstrumentationLibrary>,
}

#[pymethods]
impl Config {
    #[new]
    #[pyo3(signature = (/, file_path = None, filter = None, instrumentation_library = None))]
    const fn new(
        file_path: Option<String>,
        filter: Option<String>,
        instrumentation_library: Option<PyInstrumentationLibrary>,
    ) -> Self {
        Self {
            file_path,
            filter,
            instrumentation_library,
        }
    }
}

#[derive(Debug)]
struct OtelOtlpFile {
    writer: Option<Arc<std::sync::Mutex<std::fs::File>>>,
    resource: ResourceAttributesWithSchema,
}

impl OtelOtlpFile {
    fn new(writer: Option<std::fs::File>) -> Self {
        Self {
            writer: writer.map(|writer| Arc::new(std::sync::Mutex::new(writer))),
            resource: ResourceAttributesWithSchema::default(),
        }
    }
}

impl opentelemetry_sdk::trace::SpanExporter for OtelOtlpFile {
    fn export(
        &self,
        batch: Vec<opentelemetry_sdk::trace::SpanData>,
    ) -> impl Future<Output = OTelSdkResult> + Send {
        let writer = self.writer.clone();
        let resource_spans = group_spans_by_resource_and_scope(batch, &self.resource);

        async move {
            let traces_data = opentelemetry_proto::tonic::trace::v1::TracesData { resource_spans };
            let serialized = serde_json::to_vec(&traces_data)
                .map(|mut v| {
                    v.push(b'\n');
                    v
                })
                .map_err(|e| OTelSdkError::InternalFailure(e.to_string()))?;

            // Push file I/O to a separate thread.
            tokio::task::spawn_blocking(move || -> OTelSdkResult {
                if let Some(writer) = writer {
                    writer
                        .lock()
                        .map_err(|e| OTelSdkError::InternalFailure(e.to_string()))?
                        .write(serialized.as_slice())
                        .map(|_| ())
                        .map_err(|e| OTelSdkError::InternalFailure(e.to_string()))
                } else {
                    std::io::stdout()
                        .lock()
                        .write(serialized.as_slice())
                        .map(|_| ())
                        .map_err(|e| OTelSdkError::InternalFailure(e.to_string()))
                }
            })
            .await
            .map_err(|e| OTelSdkError::InternalFailure(e.to_string()))?
        }
    }

    fn force_flush(&mut self) -> OTelSdkResult {
        match &self.writer {
            Some(writer) => writer
                .lock()
                .map_err(|e| OTelSdkError::InternalFailure(e.to_string()))?
                .flush()
                .map_err(|e| OTelSdkError::InternalFailure(e.to_string())),
            None => std::io::stdout()
                .flush()
                .map_err(|e| OTelSdkError::InternalFailure(e.to_string())),
        }
    }

    /// Set the resource for the exporter.
    fn set_resource(&mut self, resource: &opentelemetry_sdk::Resource) {
        self.resource = ResourceAttributesWithSchema::from(resource);
    }
}

impl crate::layers::Config for Config {
    fn requires_runtime(&self) -> bool {
        false
    }

    fn build(&self, batch: bool) -> LayerBuildResult<WithShutdown> {
        use opentelemetry::trace::TracerProvider as _;
        let file = self
            .file_path
            .as_ref()
            .map(|file_path| std::fs::File::create(file_path).map_err(BuildError::from))
            .transpose()?;

        let exporter = OtelOtlpFile::new(file);
        let provider = if batch {
            opentelemetry_sdk::trace::SdkTracerProvider::builder()
                .with_batch_exporter(exporter)
                .build()
        } else {
            opentelemetry_sdk::trace::SdkTracerProvider::builder()
                .with_simple_exporter(exporter)
                .build()
        };

        let tracer = self.instrumentation_library.as_ref().map_or_else(
            || provider.tracer("pyo3_tracing_subscriber"),
            |instrumentation_library| {
                provider.tracer_with_scope(instrumentation_library.clone().into())
            },
        );
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
