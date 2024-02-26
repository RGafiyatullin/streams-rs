use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Stream, TryStream};

pub trait LatestReadyStreamExt: Stream + Sized {
    fn latest_ready(self) -> LatestReady<Self> {
        LatestReady::new(self)
    }
}

pub trait TryLatestReadyStreamExt: Stream + TryStream + Sized {
    fn try_latest_ready(self) -> TryLatestReady<Self> {
        TryLatestReady::new(self)
    }
}

/// Stream for [`latest_ready`](`LatestReadyStreamExt::latest_ready`) method.
#[derive(Debug, Clone, Copy)]
#[pin_project::pin_project]
pub struct LatestReady<Stream> {
    #[pin]
    inner: Stream,
}

/// Stream for [`try_latest_ready`](`TryLatestReady::try_latest_ready`) method.
#[derive(Debug, Clone, Copy)]
#[pin_project::pin_project]
pub struct TryLatestReady<Stream> {
    #[pin]
    inner: Stream,
}

impl<S> LatestReady<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> TryLatestReady<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Stream for LatestReady<S>
where
    S: Stream,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let mut prev_poll = Poll::Pending;
        loop {
            match this.inner.as_mut().poll_next(cx) {
                Poll::Pending => break prev_poll,
                Poll::Ready(None) => break Poll::Ready(None),
                this_poll @ Poll::Ready(Some(_)) => prev_poll = this_poll,
            }
        }
    }
}

impl<S> Stream for TryLatestReady<S>
where
    S: Stream + TryStream,
{
    type Item = Result<S::Ok, S::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let mut prev_poll = Poll::Pending;
        loop {
            match this.inner.as_mut().try_poll_next(cx) {
                Poll::Pending => break prev_poll,
                Poll::Ready(term @ (None | Some(Err(_)))) => break Poll::Ready(term),
                this_poll @ Poll::Ready(Some(Ok(_))) => prev_poll = this_poll,
            }
        }
    }
}

impl<S> LatestReadyStreamExt for S where S: Stream + Sized {}

impl<S> TryLatestReadyStreamExt for S where S: Stream + TryStream + Sized {}

#[cfg(test)]
mod tests {
    use futures::{stream, StreamExt};

    use crate::test_utils::ready_after_n_polls;

    use super::*;

    #[tokio::test]
    async fn empty_stream() {
        assert!(stream::empty::<()>()
            .latest_ready()
            .collect::<Vec<_>>()
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn an_item_is_yielded_only_if_the_upstream_returns_pending() {
        assert!(stream::iter([1, 2, 3, 4, 5, 6])
            .latest_ready()
            .collect::<Vec<_>>()
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn during_the_bursts_only_the_last_ready_element_is_yielded() {
        assert_eq!(
            stream::iter([
                [1, 2, 3, 4, 5],
                [6, 7, 8, 9, 10],
                [11, 12, 13, 14, 15],
                [16, 17, 18, 19, 20],
            ])
            .map(stream::iter)
            .then(|chunk| ready_after_n_polls(chunk, 1))
            .flatten()
            .latest_ready()
            .collect::<Vec<_>>()
            .await,
            vec![5, 10, 15]
        );
    }

    #[tokio::test]
    async fn empty_try_stream() {
        assert!(stream::empty::<Result<(), ()>>()
            .try_latest_ready()
            .collect::<Vec<_>>()
            .await
            .is_empty())
    }

    #[tokio::test]
    async fn an_item_is_yielded_only_if_the_upstream_returns_pending_try_stream() {
        assert!(stream::iter([1, 2, 3, 4, 5, 6])
            .map(Ok::<_, ()>)
            .try_latest_ready()
            .collect::<Vec<_>>()
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn during_the_bursts_only_the_last_ready_element_is_yielded_try_stream() {
        assert_eq!(
            stream::iter([
                [1, 2, 3, 4, 5],
                [6, 7, 8, 9, 10],
                [11, 12, 13, 14, 15],
                [16, 17, 18, 19, 20],
            ])
            .map(stream::iter)
            .then(|chunk| ready_after_n_polls(chunk, 1))
            .flatten()
            .map(Ok::<_, ()>)
            .try_latest_ready()
            .collect::<Vec<_>>()
            .await,
            vec![Ok(5), Ok(10), Ok(15)]
        );
    }

    #[tokio::test]
    async fn early_return() {
        assert_eq!(
            stream::iter([Ok(1), Ok(2), Err(()), Ok(3), Ok(4), Ok(5),])
                .try_latest_ready()
                .collect::<Vec<_>>()
                .await,
            vec![Err(()),]
        );
    }
}
