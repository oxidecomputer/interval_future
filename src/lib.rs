//! Implements a wrapper around non-async functions, allowing
//! them to be used within an async context.

use futures::{
    future::{FusedFuture, FutureExt},
    pin_mut, select,
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Duration,
};

/// Implements a future, using a caller-supplied polling function.
pub struct IntervalFuture<O, F>
where
    F: FnMut() -> Poll<O> + Unpin,
{
    f: F,
    completed: bool,
    waker: Option<Waker>,
}

impl<O, F> IntervalFuture<O, F>
where
    F: FnMut() -> Poll<O> + Unpin,
{
    /// Creates a new future which wraps a synchronous function `f`
    /// that may be called repeatedly on `interval_period` until
    /// it completes.
    ///
    /// Generally, when using futures, an explicit waker should
    /// be used to cause re-polling on a more precise basis, but that
    /// isn't always available. This method allows callers to create
    /// a future which will automatically be-checked with a specified
    /// regularity.
    ///
    /// # Arguments
    /// - `f'`: A function to be polled. Returns [`Poll::Ready`] with
    /// a result to complete the future, and [`Poll::Pending`] otherwise.
    /// `f` will not be re-invoked after completing.
    /// - `interval_period`: A duration of time to wait before
    /// re-invoking `f`. This is a *suggestion*, as the function may
    /// actually be re-polled more frequently.
    ///
    /// # Example
    ///
    /// ```
    /// use interval_future::IntervalFuture;
    /// use std::task::Poll;
    /// use std::time::{Duration, Instant};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let time_to_complete = Duration::from_secs(1);
    ///     let interval = Duration::from_millis(200);
    ///     let timeout = Duration::from_secs(2);
    ///
    ///     let poll_start = Instant::now();
    ///     let f = || {
    ///         let elapsed = Instant::now().duration_since(poll_start);
    ///         if elapsed > time_to_complete {
    ///             println!("Ready");
    ///             Poll::Ready(5)
    ///         } else {
    ///             println!("Not ready after {} ms", elapsed.as_millis());
    ///             Poll::Pending
    ///         }
    ///     };
    ///
    ///     let fut = IntervalFuture::wrap(f, interval);
    ///     let val = tokio::time::timeout(timeout, fut).await.unwrap();
    ///     println!("Got my value: {}", val);
    /// }
    /// ```
    pub async fn wrap(f: F, interval_period: Duration) -> O {
        let fut = IntervalFuture {
            f,
            completed: false,
            waker: None,
        };
        pin_mut!(fut);

        let mut interval_fut = tokio::time::interval(interval_period);

        // First tick completes immediately.
        interval_fut.tick().await;
        loop {
            select! {
                o = fut => return o,
                _ = interval_fut.tick().fuse() => (),
            }
            if let Some(waker) = fut.waker.take() {
                waker.wake();
            }
        }
    }
}

impl<O, F> Future for IntervalFuture<O, F>
where
    F: FnMut() -> Poll<O> + Unpin,
{
    type Output = O;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = self.get_mut();
        match (fut.f)() {
            Poll::Ready(o) => {
                fut.completed = true;
                Poll::Ready(o)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<O, F> FusedFuture for IntervalFuture<O, F>
where
    F: FnMut() -> Poll<O> + Unpin,
{
    fn is_terminated(&self) -> bool {
        self.completed
    }
}
