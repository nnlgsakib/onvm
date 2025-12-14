use futures::future::BoxFuture;
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};

/// Simple bounded-concurrency async task queue.
pub struct AsyncQueue<T> {
    tx: mpsc::UnboundedSender<T>,
}

impl<T: Send + 'static> AsyncQueue<T> {
    pub fn new<F, Fut>(concurrency: usize, worker: F) -> Self
    where
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let (tx, mut rx) = mpsc::unbounded_channel::<T>();
        let sem = Arc::new(Semaphore::new(concurrency.max(1)));
        let worker = Arc::new(move |item: T| -> BoxFuture<'static, ()> { Box::pin(worker(item)) });
        tokio::spawn({
            let worker = worker.clone();
            let sem = sem.clone();
            async move {
                while let Some(item) = rx.recv().await {
                    let permit = sem.clone().acquire_owned().await.ok();
                    let worker = worker.clone();
                    tokio::spawn(async move {
                        worker(item).await;
                        drop(permit);
                    });
                }
            }
        });
        Self { tx }
    }

    pub fn enqueue(&self, item: T) {
        let _ = self.tx.send(item);
    }
}
