use std::sync::Arc;

use std::sync::atomic::AtomicUsize;
use tokio::sync::Notify;

#[derive(Clone, Debug, Default)]
pub(crate) struct WaitGroup {
    n: Arc<AtomicUsize>,
    notify: Arc<Notify>,
}

impl WaitGroup {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            n: Arc::new(AtomicUsize::new(n)),
            notify: Arc::new(Notify::new()),
        }
    }

    pub(crate) fn add(&self, n: usize) {
        self.n.fetch_add(n, std::sync::atomic::Ordering::SeqCst);
    }

    pub(crate) fn done(&self) {
        self.n.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_one();
    }

    fn is_complete(&self) -> bool {
        self.n.load(std::sync::atomic::Ordering::SeqCst) == 0
    }

    pub(crate) async fn wait(&self) {
        while !self.is_complete() {
            self.notify.notified().await;
        }
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use tokio::time::{sleep, timeout};

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
}
