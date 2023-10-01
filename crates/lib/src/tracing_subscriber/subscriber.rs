use pyo3::prelude::*;
use rigetti_pyo3::create_init_submodule;
use tracing_subscriber::{layer::Layered, prelude::__tracing_subscriber_SubscriberExt, Registry};

#[derive(thiserror::Error, Debug)]
pub(crate) enum ShutdownError {
    #[error("failed to shutdown configured layer: {0}")]
    LayerShutdown(#[from] crate::tracing_subscriber::layers::ShutdownError),
}

type ShutdownResult<T> = Result<T, ShutdownError>;

#[derive(thiserror::Error, Debug)]
pub(crate) enum BuildError {
    #[error("failed to build layer: {0}")]
    LayerBuild(#[from] crate::tracing_subscriber::layers::BuildError),
}

#[derive(thiserror::Error, Debug)]
#[error("{message}")]
pub(crate) struct CustomError {
    message: String,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

pub(crate) type Shutdown = Box<
    dyn (FnOnce() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = ShutdownResult<()>> + Send + Sync>,
        >) + Send
        + Sync,
>;

type SubscriberBuildResult<T> = Result<T, BuildError>;

pub(crate) trait Config: BoxDynConfigClone + Send + Sync {
    fn build(&self, batch: bool) -> SubscriberBuildResult<WithShutdown>;
}

pub(crate) trait BoxDynConfigClone {
    fn clone_box(&self) -> Box<dyn Config>;
}

impl<T> BoxDynConfigClone for T
where
    T: 'static + Config + Clone,
{
    fn clone_box(&self) -> Box<dyn Config> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Config> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

pub(crate) trait SendSyncSubscriber: tracing::subscriber::Subscriber + Send + Sync {}

impl<L, I> SendSyncSubscriber for Layered<L, I>
where
    L: tracing_subscriber::Layer<I> + Send + Sync,
    I: tracing::Subscriber + Send + Sync,
{
}

pub(crate) struct WithShutdown {
    pub(crate) subscriber: Option<Box<dyn SendSyncSubscriber>>,
    pub(crate) shutdown: Shutdown,
}

impl core::fmt::Debug for WithShutdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "WithShutdown subscriber: {}, shutdown: Shutdown",
            &self
                .subscriber
                .as_ref()
                .map_or("None", |_| "Some(Box<dyn SendSyncSubscriber>)"),
        )
    }
}

#[pyclass(name = "Config")]
#[derive(Clone)]
pub(crate) struct PyConfig {
    pub(crate) subscriber_config: Box<dyn Config>,
}

impl core::fmt::Debug for PyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PyConfig {{ subscriber_config: Box<dyn Config> }}")
    }
}

#[pymethods]
impl PyConfig {
    #[new]
    fn new(layer: super::layers::PyConfig) -> Self {
        Self {
            subscriber_config: Box::new(TracingSubscriberRegistryConfig {
                layer_config: layer.layer_config,
            }),
        }
    }
}

#[derive(Clone)]
pub(super) struct TracingSubscriberRegistryConfig {
    pub(super) layer_config: Box<dyn super::layers::Config>,
}

impl Config for TracingSubscriberRegistryConfig {
    fn build(&self, batch: bool) -> SubscriberBuildResult<WithShutdown> {
        let layer = self.layer_config.clone().build(batch)?;
        let subscriber = Registry::default().with(layer.layer);
        let shutdown = layer.shutdown;
        Ok(WithShutdown {
            subscriber: Some(Box::new(subscriber)),
            shutdown: Box::new(move || {
                Box::pin(async move {
                    shutdown().await?;
                    Ok(())
                })
            }),
        })
    }
}

create_init_submodule! {
    classes: [
        PyConfig
    ],
}
