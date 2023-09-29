use pyo3::prelude::*;
use tracing_subscriber::{layer::Layered, prelude::__tracing_subscriber_SubscriberExt, Registry};

#[derive(thiserror::Error, Debug)]
pub(crate) enum ShutdownError {
    #[error("failed to shutdown configured layer: {0}")]
    LayerShutdown(#[from] crate::export::layer::LayerError),
}

type ShutdownResult<T> = Result<T, ShutdownError>;

#[derive(thiserror::Error, Debug)]
pub(crate) enum BuildError {
    #[error("failed to build layer: {0}")]
    LayerBuild(#[from] crate::export::layer::BuildError),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("failed to build layer: {0}")]
    BuildLayer(#[from] super::layer::LayerError),
    #[error(transparent)]
    Custom(#[from] CustomError),
    #[error("failed to shutdown subscriber: {0}")]
    SubscriberShutdown(#[from] ShutdownError),
}

#[derive(thiserror::Error, Debug)]
#[error("{message}")]
pub struct CustomError {
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

pub type SubscriberResult<T> = Result<T, Error>;
type SubscriberBuildResult<T> = Result<T, BuildError>;

pub trait Config: BoxDynConfigClone + Send + Sync {
    fn build(&self, batch: bool) -> SubscriberBuildResult<SubscriberWithShutdown>;
}

#[pyclass(name = "Config")]
#[derive(Clone)]
pub struct PyConfig {
    pub(crate) subscriber_config: Box<dyn Config>,
}

trait BoxDynConfigClone {
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

pub struct SubscriberWithShutdown {
    pub(crate) subscriber: Option<Box<dyn SendSyncSubscriber>>,
    pub(crate) shutdown: Shutdown,
}

#[derive(Clone)]
pub(crate) struct TracingSubscriberRegistryConfig {
    layer_config: Box<dyn super::layer::Config>,
}

impl Config for TracingSubscriberRegistryConfig {
    fn build(&self, batch: bool) -> SubscriberBuildResult<SubscriberWithShutdown> {
        let layer = self.layer_config.clone().build(batch)?;
        let subscriber = Registry::default().with(layer.layer);
        let shutdown = layer.shutdown;
        Ok(SubscriberWithShutdown {
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
