use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Stream, TryStream};

pub trait ZipBiasedStreamExt: Stream + Sized {
    fn zip_biased<R>(self, right: R) -> ZipBiased<Self, R, Self::Item>
    where
        R: Stream,
    {
        ZipBiased::new(self, right)
    }
}

pub trait TryZipBiasedStreamExt: Stream + TryStream + Sized {
    fn try_zip_biased<R>(self, right: R) -> TryZipBiased<Self, R, Self::Ok>
    where
        R: Stream + TryStream<Error = Self::Error>,
    {
        TryZipBiased::new(self, right)
    }
}

#[derive(Debug, Clone, Copy)]
#[pin_project::pin_project]
pub struct ZipBiased<L, R, LI> {
    #[pin]
    left: L,
    #[pin]
    right: R,

    left_poll: Poll<Option<LI>>,
}

#[derive(Debug, Clone, Copy)]
#[pin_project::pin_project]
pub struct TryZipBiased<L, R, LI> {
    #[pin]
    left: L,
    #[pin]
    right: R,

    left_poll: Poll<Option<LI>>,
}

impl<L, R, LI> ZipBiased<L, R, LI> {
    pub fn new(left: L, right: R) -> Self {
        Self {
            left,
            right,
            left_poll: Poll::Pending,
        }
    }
}

impl<L, R, LI> TryZipBiased<L, R, LI> {
    pub fn new(left: L, right: R) -> Self {
        Self {
            left,
            right,
            left_poll: Poll::Pending,
        }
    }
}

impl<L, R> Stream for ZipBiased<L, R, L::Item>
where
    L: Stream,
    R: Stream,
{
    type Item = (L::Item, R::Item);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::task::ready;

        let mut this = self.project();

        Poll::Ready(loop {
            match this.left_poll {
                Poll::Pending => {
                    *this.left_poll = Poll::Ready(ready!(this.left.as_mut().poll_next(cx)))
                }
                Poll::Ready(None) => break None,
                Poll::Ready(some_left @ Some(_)) => {
                    let right_opt = ready!(this.right.as_mut().poll_next(cx));
                    let some_left = some_left.take();
                    *this.left_poll = Poll::Pending;
                    break some_left.zip(right_opt);
                }
            }
        })
    }
}

impl<L, R> Stream for TryZipBiased<L, R, L::Ok>
where
    L: Stream + TryStream,
    R: Stream + TryStream<Error = L::Error>,
    L: Stream<Item = Result<L::Ok, L::Error>>,
    R: Stream<Item = Result<R::Ok, L::Error>>,
{
    type Item = Result<(L::Ok, R::Ok), L::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::task::ready;

        let mut this = self.project();

        Poll::Ready(loop {
            match this.left_poll {
                Poll::Pending => match ready!(this.left.as_mut().poll_next(cx)).transpose() {
                    Err(reason) => break Some(Err(reason)),
                    Ok(left_opt) => *this.left_poll = Poll::Ready(left_opt),
                },
                Poll::Ready(None) => break None,
                Poll::Ready(some_left @ Some(_)) => {
                    match ready!(this.right.as_mut().poll_next(cx)).transpose() {
                        Err(reason) => break Some(Err(reason)),
                        Ok(right_opt) => break some_left.take().zip(right_opt).map(Ok),
                    }
                }
            }
        })
    }
}

impl<L> ZipBiasedStreamExt for L where L: Stream + Sized {}
impl<L> TryZipBiasedStreamExt for L where L: Stream + TryStream + Sized {}

#[cfg(test)]
mod tests {
    use futures::stream;
    use futures::StreamExt;

    use super::*;

    #[tokio::test]
    async fn left_empty() {
        let left = stream::empty::<()>();
        let right = stream::repeat(());

        assert_eq!(left.zip_biased(right).collect::<Vec<_>>().await, vec![]);
    }

    #[tokio::test]
    async fn right_empty() {
        let left = stream::repeat(());
        let right = stream::empty::<()>();

        assert_eq!(left.zip_biased(right).collect::<Vec<_>>().await, vec![]);
    }
}
