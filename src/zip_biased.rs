use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::Stream;

pub trait ZipBiasedStreamExt: Stream + Sized {
    fn zip_biased<R>(self, right: R) -> ZipBiased<Self, R, Self::Item>
    where
        R: Stream,
    {
        ZipBiased::new(self, right)
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

impl<L, R, LI> ZipBiased<L, R, LI> {
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
                Poll::Ready(None) => break None,
                Poll::Ready(some_left @ Some(_)) => {
                    let right_opt = ready!(this.right.as_mut().poll_next(cx));
                    let some_left = some_left.take();
                    *this.left_poll = Poll::Pending;
                    break some_left.zip(right_opt);
                }
                Poll::Pending => {
                    *this.left_poll = Poll::Ready(ready!(this.left.as_mut().poll_next(cx)));
                }
            }
        })
    }
}

impl<L> ZipBiasedStreamExt for L where L: Stream + Sized {}
