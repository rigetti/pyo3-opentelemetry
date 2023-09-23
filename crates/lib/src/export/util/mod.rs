use std::sync::Arc;

use once_cell::sync::OnceCell;
use pyo3::prelude::*;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::TracerProvider;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::Notify;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

use std::sync::atomic::AtomicUsize;

static RUNTIME: OnceCell<Runtime> = OnceCell::new();
static BATCH_EXPORT_PROCESS: OnceCell<Arc<Notify>> = OnceCell::new();

fn init_runtime() -> Runtime {
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
}

pub(super) fn stop(py: Python<'_>) -> PyResult<&PyAny> {
    let batch_process = BATCH_EXPORT_PROCESS.get().unwrap();
    batch_process.notify_one();
    pyo3_asyncio::tokio::future_into_py(py, async {
        let batch_process = BATCH_EXPORT_PROCESS.get().unwrap();
        batch_process.notified().await;
        println!("TOTALLY FLUSHED");
        Ok(())
    })
}

pub(super) fn start_tracer<F>(f: F)
where
    F: FnOnce() -> (TracerProvider, opentelemetry_sdk::trace::Tracer) + Send + 'static,
{
    let notify = Arc::new(Notify::new());
    let rt = init_runtime();
    RUNTIME.set(rt).unwrap();
    BATCH_EXPORT_PROCESS.set(notify.clone()).unwrap();
    let rt = RUNTIME.get().unwrap();
    rt.spawn(async move {
        let (provider, tracer) = f();
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

        // Use the tracing subscriber `Registry`, or any other subscriber
        // that impls `LookupSpan`
        let subscriber = Registry::default().with(telemetry);
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting tracing default failed");
        println!("stdout tracer a");
        notify.notified().await;
        println!("stdout tracer b");
        provider.force_flush();
        notify.notify_one();
        println!("stdout tracer c (flushed)");
    });
}

#[derive(Clone, Debug, Default)]
pub(super) struct WaitGroup {
    n: Arc<AtomicUsize>,
    notify: Arc<Notify>,
}

impl WaitGroup {
    pub(super) fn new(n: usize) -> Self {
        Self {
            n: Arc::new(AtomicUsize::new(n)),
            notify: Arc::new(Notify::new()),
        }
    }

    pub(super) fn add(&self, n: usize) {
        self.n.fetch_add(n, std::sync::atomic::Ordering::SeqCst);
    }

    pub(super) fn done(&self) {
        self.n.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_one();
    }

    fn is_complete(&self) -> bool {
        self.n.load(std::sync::atomic::Ordering::SeqCst) == 0
    }

    pub(super) async fn wait(&self) {
        while !self.is_complete() {
            self.notify.notified().await;
        }
    }
}

#[cfg(test)]
mod test {
    use std::{sync::Arc, time::Duration};

    use tokio::{
        sync::Notify,
        time::{sleep, timeout},
    };

    use super::WaitGroup;

    #[tokio::test]
    async fn test_wait_group_completion_sync() {
        let wg = WaitGroup::new(0);
        assert!(wg.is_complete());
        wg.add(10);
        assert!(!wg.is_complete());
        for _ in 0..9 {
            wg.done();
        }
        assert!(!wg.is_complete());
        wg.done();
        assert!(wg.is_complete());

        wg.add(1);
        assert!(!wg.is_complete());
        wg.done();
        assert!(wg.is_complete());
    }

    #[tokio::test]
    async fn test_wait_group_completion_async() {
        let wg = WaitGroup::new(0);
        wg.add(5);
        for _ in 0..5 {
            let wg = wg.clone();
            tokio::spawn(async move {
                sleep(Duration::from_millis(10)).await;
                wg.done();
            });
        }
        assert!(!wg.is_complete());
        assert_eq!(timeout(Duration::from_millis(500), wg.wait()).await, Ok(()));
        assert!(wg.is_complete());

        // ensure wait group is reusable
        wg.add(1);
        assert!(!wg.is_complete());
        wg.done();
        assert!(wg.is_complete());
    }

    #[tokio::test]
    async fn test_notify() {
        let notify = Arc::new(Notify::new());
        let notify2 = notify.clone();

        let handle = tokio::spawn(async move {
            notify2.notified().await;
            println!("received notification");
            notify2.notify_one();
        });

        println!("sending notification");
        notify.notify_one();
        timeout(Duration::from_millis(500), handle)
            .await
            .unwrap()
            .unwrap();
        timeout(Duration::from_millis(10), notify.notified())
            .await
            .unwrap();
    }
}
