//! When a review's diff is generated: one at a time across the daemon,
//! because each walks a whole working tree and several at once only make
//! every one of them late, and only the newest a client has asked for of
//! each side, because a side asked for again replaces the one before it.
//! A review opens by asking for its unstaged and staged sides together, so
//! one side never replaces the other.

use std::sync::Arc;

use argus_protocol::ReviewBase;

use tokio::sync::Semaphore;
use tokio::task::{JoinError, JoinHandle};

/// The daemon's one turn at generating a review, shared by every client.
pub struct ReviewQueue {
    turn: Arc<Semaphore>,
}

impl Default for ReviewQueue {
    fn default() -> Self {
        Self {
            turn: Arc::new(Semaphore::new(1)),
        }
    }
}

impl ReviewQueue {
    /// One client's place in the queue, held for as long as its connection.
    pub fn lane(&self) -> ReviewLane {
        ReviewLane {
            turn: self.turn.clone(),
            pending: Vec::new(),
        }
    }
}

/// The reviews one client is waiting on, at most one per side. Dropping
/// it, as a closing connection does, drops the requests with it.
pub struct ReviewLane {
    turn: Arc<Semaphore>,
    pending: Vec<(ReviewBase, JoinHandle<()>)>,
}

impl ReviewLane {
    /// Queues `generate` for the daemon's turn and hands its result to
    /// `deliver`, replacing whatever this lane asked for before of `side`.
    ///
    /// A replaced request that has not started never runs. One already
    /// generating runs to the end on its blocking thread, still holding the
    /// turn — a thread cannot be stopped partway — but its answer is never
    /// delivered.
    pub fn submit<R: Send + 'static>(
        &mut self,
        side: ReviewBase,
        generate: impl FnOnce() -> R + Send + 'static,
        deliver: impl FnOnce(Result<R, JoinError>) + Send + 'static,
    ) {
        self.pending.retain(|(asked, task)| {
            let replaced = *asked == side;
            if replaced {
                task.abort();
            }
            !replaced && !task.is_finished()
        });
        let turn = self.turn.clone();
        let task = tokio::spawn(async move {
            let Ok(permit) = turn.acquire_owned().await else {
                return;
            };
            let generated = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                generate()
            })
            .await;
            deliver(generated);
        });
        self.pending.push((side, task));
    }
}

impl Drop for ReviewLane {
    fn drop(&mut self) {
        for (_, task) in self.pending.drain(..) {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::*;

    /// A review that generates only once `release` is sent, and says when
    /// it starts.
    fn held(
        started: &mpsc::UnboundedSender<u32>,
        id: u32,
    ) -> (impl FnOnce() -> u32 + Send + 'static, std::sync::mpsc::Sender<()>) {
        let (release, wait) = std::sync::mpsc::channel::<()>();
        let started = started.clone();
        let generate = move || {
            let _ = started.send(id);
            let _ = wait.recv();
            id
        };
        (generate, release)
    }

    fn deliver_to(done: &mpsc::UnboundedSender<u32>) -> impl FnOnce(Result<u32, JoinError>) + Send {
        let done = done.clone();
        move |result| {
            let _ = done.send(result.expect("the review did not panic"));
        }
    }

    async fn within<T>(rx: &mut mpsc::UnboundedReceiver<T>) -> Option<T> {
        tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.ok().flatten()
    }

    async fn nothing_from<T: std::fmt::Debug>(rx: &mut mpsc::UnboundedReceiver<T>) {
        let late = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(late.is_err(), "{late:?}");
    }

    #[tokio::test]
    async fn only_one_review_generates_at_a_time_across_clients() {
        let queue = ReviewQueue::default();
        let (started_tx, mut started) = mpsc::unbounded_channel();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let (mut first, mut second) = (queue.lane(), queue.lane());

        let (generate, release_first) = held(&started_tx, 1);
        first.submit(ReviewBase::Unstaged, generate, deliver_to(&done_tx));
        assert_eq!(within(&mut started).await, Some(1));
        let (generate, release_second) = held(&started_tx, 2);
        second.submit(ReviewBase::Unstaged, generate, deliver_to(&done_tx));
        nothing_from(&mut started).await;

        release_first.send(()).unwrap();
        assert_eq!(within(&mut done).await, Some(1));
        assert_eq!(within(&mut started).await, Some(2), "the next takes the turn");
        release_second.send(()).unwrap();
        assert_eq!(within(&mut done).await, Some(2));
    }

    #[tokio::test]
    async fn a_newer_request_replaces_an_older_queued_one() {
        let queue = ReviewQueue::default();
        let (started_tx, mut started) = mpsc::unbounded_channel();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let (mut other, mut client) = (queue.lane(), queue.lane());
        let (generate, release) = held(&started_tx, 0);
        other.submit(ReviewBase::Unstaged, generate, |_| {});
        assert_eq!(within(&mut started).await, Some(0));

        client.submit(ReviewBase::Unstaged, move || 1, deliver_to(&done_tx));
        client.submit(ReviewBase::Unstaged, move || 2, deliver_to(&done_tx));
        release.send(()).unwrap();

        assert_eq!(within(&mut done).await, Some(2));
        nothing_from(&mut done).await;
    }

    #[tokio::test]
    async fn a_request_for_another_side_does_not_replace_the_first() {
        let queue = ReviewQueue::default();
        let (started_tx, mut started) = mpsc::unbounded_channel();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let mut other = queue.lane();
        let (generate, release) = held(&started_tx, 0);
        other.submit(ReviewBase::Unstaged, generate, |_| {});
        assert_eq!(within(&mut started).await, Some(0));

        let mut client = queue.lane();
        client.submit(ReviewBase::Unstaged, move || 1, deliver_to(&done_tx));
        client.submit(ReviewBase::Staged, move || 2, deliver_to(&done_tx));
        release.send(()).unwrap();

        let mut delivered = [within(&mut done).await, within(&mut done).await];
        delivered.sort();
        assert_eq!(delivered, [Some(1), Some(2)]);
    }

    #[tokio::test]
    async fn a_replaced_review_already_generating_is_never_delivered() {
        let queue = ReviewQueue::default();
        let (started_tx, mut started) = mpsc::unbounded_channel();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let mut client = queue.lane();
        let (generate, release) = held(&started_tx, 1);
        client.submit(ReviewBase::Unstaged, generate, deliver_to(&done_tx));
        assert_eq!(within(&mut started).await, Some(1));

        client.submit(ReviewBase::Unstaged, move || 2, deliver_to(&done_tx));
        release.send(()).unwrap();

        assert_eq!(within(&mut done).await, Some(2));
        nothing_from(&mut done).await;
    }

    #[tokio::test]
    async fn a_client_that_goes_takes_its_queued_review_with_it() {
        let queue = ReviewQueue::default();
        let (started_tx, mut started) = mpsc::unbounded_channel();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let mut other = queue.lane();
        let (generate, release) = held(&started_tx, 0);
        other.submit(ReviewBase::Unstaged, generate, |_| {});
        assert_eq!(within(&mut started).await, Some(0));

        let mut client = queue.lane();
        client.submit(ReviewBase::Unstaged, move || 1, deliver_to(&done_tx));
        drop(client);
        release.send(()).unwrap();

        nothing_from(&mut done).await;
    }
}
