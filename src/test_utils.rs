use std::future;
use std::future::Future;
use std::task::Poll;

use futures::FutureExt;

pub fn ready_after_n_polls<V>(value: V, mut polls: usize) -> impl Future<Output = V> {
    let mut value = Some(value);
    future::poll_fn(move |cx| {
        if let Some(next) = polls.checked_sub(1) {
            polls = next;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(value.take().expect("value has been taken"))
        }
    })
    .fuse()
}
