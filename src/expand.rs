use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Stream, TryStream};

pub trait ExpandStreamExt
where
    Self: Stream + Sized,
    Self::Item: Clone,
{
    /// Allow for a fatster downstream by cloning the last produced element whenever the upstream is pending.
    fn expand(self) -> Expand<Self, Self::Item> {
        Expand::new(self)
    }
}

pub trait TryExpandStreamExt
where
    Self: Stream + TryStream + Sized,
    Self::Ok: Clone,
{
    /// Similar to [`expand`](`ExpandStreamExt::expand`) but for `TryStream`.
    fn try_expand(self) -> TryExpand<Self, Self::Ok> {
        TryExpand::new(self)
    }
}

/// Stream for [`expand`](`ExpandStreamExt::expand`) method.
#[derive(Debug, Clone, Copy)]
#[pin_project::pin_project]
pub struct Expand<Stream, Item> {
    #[pin]
    inner: Stream,

    last_poll: Poll<Option<Item>>,
}

/// Stream for [`try_expand`](`TryExpandStreamExt::try_expand`) method.
#[derive(Debug, Clone, Copy)]
#[pin_project::pin_project]
pub struct TryExpand<Stream, Ok> {
    #[pin]
    inner: Stream,
    terminated: bool,

    last_poll: Poll<Option<Ok>>,
}

impl<S> Expand<S, S::Item>
where
    S: Stream,
    S::Item: Clone,
{
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            last_poll: Poll::Pending,
        }
    }
}

impl<S> TryExpand<S, S::Ok>
where
    S: Stream + TryStream,
    S::Ok: Clone,
{
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            terminated: false,
            last_poll: Poll::Pending,
        }
    }
}

impl<S> Stream for Expand<S, S::Item>
where
    S: Stream,
    S::Item: Clone,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let this_poll = this.inner.as_mut().poll_next(cx);

        match (this_poll, this.last_poll) {
            (Poll::Pending, Poll::Pending) => Poll::Pending,
            (Poll::Pending, Poll::Ready(last_ready)) => Poll::Ready(last_ready.clone()),
            (Poll::Ready(newer), last_poll) => {
                *last_poll = Poll::Ready(newer);
                last_poll.clone()
            }
        }
    }
}

impl<S> Stream for TryExpand<S, S::Ok>
where
    S: Stream + TryStream,
    S::Ok: Clone,
{
    type Item = Result<S::Ok, S::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.terminated {
            return Poll::Ready(None);
        }

        let mut this = self.project();
        let this_poll = this.inner.as_mut().try_poll_next(cx);

        match (this_poll, this.last_poll) {
            (Poll::Pending, Poll::Pending) => Poll::Pending,
            (Poll::Pending, Poll::Ready(last_ready)) => {
                Poll::Ready(last_ready.as_ref().cloned().map(Ok))
            }
            (Poll::Ready(Some(Ok(newer))), last_poll) => {
                *last_poll = Poll::Ready(Some(newer));
                last_poll.clone().map(|opt| opt.map(Ok))
            }
            (Poll::Ready(term @ (None | Some(Err(_)))), last_poll) => {
                *last_poll = Poll::Ready(None);
                *this.terminated = true;
                Poll::Ready(term)
            }
        }
    }
}

impl<S> ExpandStreamExt for S
where
    S: Stream + Sized,
    S::Item: Clone,
{
}

impl<S> TryExpandStreamExt for S
where
    S: Stream + TryStream + Sized,
    S::Ok: Clone,
{
}

#[cfg(test)]
mod tests {
    use futures::{stream, StreamExt};

    use crate::test_utils::ready_after_n_polls;

    use super::*;

    #[tokio::test]
    async fn empty_stream_immediately_ends() {
        assert!(stream::empty::<()>()
            .expand()
            .collect::<Vec<_>>()
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn acts_as_normal_stream() {
        assert_eq!(
            stream::iter([1, 2, 3, 4, 5])
                .expand()
                .collect::<Vec<_>>()
                .await,
            vec![1, 2, 3, 4, 5]
        );
    }

    #[tokio::test]
    async fn repeats_the_last_item_if_not_termiated() {
        assert_eq!(
            stream::iter([1, 2, 3, 4, 5])
                .chain(stream::pending())
                .expand()
                .take(7)
                .collect::<Vec<_>>()
                .await,
            vec![1, 2, 3, 4, 5, 5, 5]
        );
    }

    #[tokio::test]
    async fn repeats_the_last_item_if_poll_is_pending() {
        assert_eq!(
            stream::iter([1, 2, 3, 4, 5])
                .chain(stream::once(ready_after_n_polls(6, 1)))
                .expand()
                .collect::<Vec<_>>()
                .await,
            vec![1, 2, 3, 4, 5, 5, 6]
        );
    }

    #[tokio::test]
    async fn repeats_the_last_item_while_item_is_pending() {
        assert_eq!(
            stream::iter([1, 2, 3, 4, 5])
                .chain(stream::once(ready_after_n_polls(6, 3)))
                .expand()
                .collect::<Vec<_>>()
                .await,
            vec![1, 2, 3, 4, 5, 5, 5, 5, 6]
        );
    }

    #[tokio::test]
    async fn try_stream_normal_termination() {
        assert_eq!(
            stream::iter([Ok::<_, ()>(1), Ok(2), Ok(3), Ok(4), Ok(5)])
                .try_expand()
                .collect::<Vec<_>>()
                .await,
            vec![Ok(1), Ok(2), Ok(3), Ok(4), Ok(5)]
        );
    }

    #[tokio::test]
    async fn try_stream_early_termination() {
        assert_eq!(
            stream::iter([Ok::<_, ()>(1), Ok(2), Err(()), Ok(4), Ok(5)])
                .try_expand()
                .collect::<Vec<_>>()
                .await,
            vec![Ok(1), Ok(2), Err(())]
        );
    }
}
