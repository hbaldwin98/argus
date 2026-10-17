//! When a review's diff is generated and which asks it answers: one at a
//! time across the daemon, because each walks a whole working tree and
//! several at once only make every one of them late. A client holds one
//! place in that turn and reads its asks in the order it made them, so a
//! client with many asks never queues another behind all of them. An ask
//! it repeats while its twin still waits is answered by that one diff,
//! read after both; any other is read on its own, so two commits or two
//! sides asked back to back never cancel each other. Every ask is
//! answered, so no client waits on one that went nowhere.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use argus_protocol::{CheckoutId, ReviewBase};

use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

/// What one ask for a review names. Two that name the same are answered by
/// the same diff, so a commit is named by its commit whatever side it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAsk {
    checkout: CheckoutId,
    base: ReviewBase,
    commit: Option<String>,
}

impl ReviewAsk {
    pub fn new(checkout: CheckoutId, base: ReviewBase, commit: Option<String>) -> ReviewAsk {
        let base = if commit.is_some() { ReviewBase::Commit } else { base };
        ReviewAsk { checkout, base, commit }
    }
}

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
    pub fn lane<R>(&self) -> ReviewLane<R> {
        ReviewLane {
            turn: self.turn.clone(),
            queue: Arc::new(Mutex::new(Queue {
                waiting: VecDeque::new(),
                driving: false,
            })),
            driver: None,
        }
    }
}

type Deliver<R> = Box<dyn FnOnce(Result<R, String>) + Send>;
type Generate<R> = Box<dyn FnOnce() -> R + Send>;

/// An ask not yet read, and every ask its one diff will answer.
struct Waiting<R> {
    ask: ReviewAsk,
    generate: Generate<R>,
    answers: Vec<Deliver<R>>,
}

/// What a lane has not yet read, oldest first, and whether its driver is
/// running: it is whenever `waiting` is not empty.
struct Queue<R> {
    waiting: VecDeque<Waiting<R>>,
    driving: bool,
}

fn lock<R>(queue: &Mutex<Queue<R>>) -> MutexGuard<'_, Queue<R>> {
    queue.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The reviews one client is waiting on, read in the order asked by one
/// driver, so the client holds one place in the daemon's turn however
/// many it asked for. Dropping it, as a closing connection does, drops
/// the asks with it unanswered: there is no one left to answer.
pub struct ReviewLane<R> {
    turn: Arc<Semaphore>,
    queue: Arc<Mutex<Queue<R>>>,
    driver: Option<JoinHandle<()>>,
}

impl<R: Clone + Send + 'static> ReviewLane<R> {
    /// Answers `ask` through `deliver`, exactly once: with the diff of an
    /// identical ask still waiting in this lane when there is one, else
    /// with what `generate` reads when the lane's turn reaches it. A
    /// generation that panics answers every ask it holds with why.
    pub fn submit(
        &mut self,
        ask: ReviewAsk,
        generate: impl FnOnce() -> R + Send + 'static,
        deliver: impl FnOnce(Result<R, String>) + Send + 'static,
    ) {
        let mut queue = lock(&self.queue);
        if let Some(twin) = queue.waiting.iter_mut().find(|w| w.ask == ask) {
            twin.answers.push(Box::new(deliver));
            return;
        }
        queue.waiting.push_back(Waiting {
            ask,
            generate: Box::new(generate),
            answers: vec![Box::new(deliver)],
        });
        if !queue.driving {
            queue.driving = true;
            self.driver = Some(tokio::spawn(drive(self.turn.clone(), self.queue.clone())));
        }
    }
}

/// Reads the lane's asks one turn at a time, oldest first, and stops once
/// none is left. Emptiness is decided under the lock `submit` pushes
/// under, so an ask is never left with no driver to read it.
async fn drive<R: Clone + Send + 'static>(turn: Arc<Semaphore>, queue: Arc<Mutex<Queue<R>>>) {
    loop {
        {
            let mut queue = lock(&queue);
            if queue.waiting.is_empty() {
                queue.driving = false;
                return;
            }
        }
        let Ok(permit) = turn.clone().acquire_owned().await else {
            return;
        };
        // Only this driver takes from the queue, so it is still not empty.
        let next = lock(&queue).waiting.pop_front();
        let Some(Waiting { generate, answers, .. }) = next else {
            continue;
        };
        let generated = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            generate()
        })
        .await
        .map_err(|error| error.to_string());
        answer(answers, generated);
    }
}

/// Each ask its copy, the last the original, so a lone ask costs no clone.
fn answer<R: Clone>(mut asks: Vec<Deliver<R>>, generated: Result<R, String>) {
    let last = asks.pop();
    for deliver in asks {
        deliver(generated.clone());
    }
    if let Some(deliver) = last {
        deliver(generated);
    }
}

impl<R> Drop for ReviewLane<R> {
    fn drop(&mut self) {
        if let Some(driver) = self.driver.take() {
            driver.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::*;

    fn ask(checkout: u64, base: ReviewBase, commit: Option<&str>) -> ReviewAsk {
        ReviewAsk::new(CheckoutId(checkout), base, commit.map(str::to_string))
    }

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

    /// A review that says when it starts and generates at once.
    fn said(started: &mpsc::UnboundedSender<u32>, id: u32) -> impl FnOnce() -> u32 + Send + 'static {
        let started = started.clone();
        move || {
            let _ = started.send(id);
            id
        }
    }

    fn deliver_to(
        done: &mpsc::UnboundedSender<(&'static str, u32)>,
        tag: &'static str,
    ) -> impl FnOnce(Result<u32, String>) + Send {
        let done = done.clone();
        move |result| {
            let _ = done.send((tag, result.expect("the review did not panic")));
        }
    }

    async fn within<T>(rx: &mut mpsc::UnboundedReceiver<T>) -> Option<T> {
        tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.ok().flatten()
    }

    async fn nothing_from<T: std::fmt::Debug>(rx: &mut mpsc::UnboundedReceiver<T>) {
        let late = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(late.is_err(), "{late:?}");
    }

    /// Another client's review holding the daemon's turn until released.
    async fn hold_the_turn(queue: &ReviewQueue) -> (ReviewLane<u32>, std::sync::mpsc::Sender<()>) {
        let (started_tx, mut started) = mpsc::unbounded_channel();
        let mut other = queue.lane();
        let (generate, release) = held(&started_tx, 0);
        other.submit(ask(99, ReviewBase::Unstaged, None), generate, |_| {});
        assert_eq!(within(&mut started).await, Some(0));
        (other, release)
    }

    #[test]
    fn an_ask_names_a_commit_by_its_commit_whatever_side_it_says() {
        assert_eq!(
            ask(1, ReviewBase::Unstaged, Some("a")),
            ask(1, ReviewBase::Commit, Some("a"))
        );
        assert_ne!(
            ask(1, ReviewBase::Unstaged, Some("a")),
            ask(1, ReviewBase::Unstaged, Some("b"))
        );
    }

    #[tokio::test]
    async fn only_one_review_generates_at_a_time_across_clients() {
        let queue = ReviewQueue::default();
        let (started_tx, mut started) = mpsc::unbounded_channel();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let (mut first, mut second) = (queue.lane(), queue.lane());

        let (generate, release_first) = held(&started_tx, 1);
        first.submit(ask(1, ReviewBase::Unstaged, None), generate, deliver_to(&done_tx, "a"));
        assert_eq!(within(&mut started).await, Some(1));
        let (generate, release_second) = held(&started_tx, 2);
        second.submit(ask(1, ReviewBase::Unstaged, None), generate, deliver_to(&done_tx, "b"));
        nothing_from(&mut started).await;

        release_first.send(()).unwrap();
        assert_eq!(within(&mut done).await, Some(("a", 1)));
        assert_eq!(within(&mut started).await, Some(2), "the next takes the turn");
        release_second.send(()).unwrap();
        assert_eq!(within(&mut done).await, Some(("b", 2)));
    }

    #[tokio::test]
    async fn an_ask_repeated_while_its_twin_waits_is_answered_by_the_one_diff() {
        let queue = ReviewQueue::default();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let (_other, release) = hold_the_turn(&queue).await;

        let mut client = queue.lane();
        client.submit(ask(1, ReviewBase::Unstaged, None), || 1, deliver_to(&done_tx, "a"));
        client.submit(ask(1, ReviewBase::Unstaged, None), || 2, deliver_to(&done_tx, "b"));
        release.send(()).unwrap();

        let mut delivered = [within(&mut done).await, within(&mut done).await];
        delivered.sort();
        assert_eq!(delivered, [Some(("a", 1)), Some(("b", 1))]);
        nothing_from(&mut done).await;
    }

    #[tokio::test]
    async fn an_ask_for_another_side_is_generated_on_its_own() {
        let queue = ReviewQueue::default();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let (_other, release) = hold_the_turn(&queue).await;

        let mut client = queue.lane();
        client.submit(ask(1, ReviewBase::Unstaged, None), || 1, deliver_to(&done_tx, "a"));
        client.submit(ask(1, ReviewBase::Staged, None), || 2, deliver_to(&done_tx, "b"));
        release.send(()).unwrap();

        let mut delivered = [within(&mut done).await, within(&mut done).await];
        delivered.sort();
        assert_eq!(delivered, [Some(("a", 1)), Some(("b", 2))]);
    }

    #[tokio::test]
    async fn an_ask_repeated_after_its_twin_started_is_read_again_and_both_are_answered() {
        let queue = ReviewQueue::default();
        let (started_tx, mut started) = mpsc::unbounded_channel();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let mut client = queue.lane();
        let (generate, release) = held(&started_tx, 1);
        client.submit(ask(1, ReviewBase::Unstaged, None), generate, deliver_to(&done_tx, "a"));
        assert_eq!(within(&mut started).await, Some(1));

        client.submit(ask(1, ReviewBase::Unstaged, None), || 2, deliver_to(&done_tx, "b"));
        release.send(()).unwrap();

        assert_eq!(within(&mut done).await, Some(("a", 1)));
        assert_eq!(within(&mut done).await, Some(("b", 2)));
    }

    #[tokio::test]
    async fn a_client_that_goes_takes_its_queued_review_with_it() {
        let queue = ReviewQueue::default();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let (_other, release) = hold_the_turn(&queue).await;

        let mut client = queue.lane();
        client.submit(ask(1, ReviewBase::Unstaged, None), || 1, deliver_to(&done_tx, "a"));
        drop(client);
        release.send(()).unwrap();

        nothing_from(&mut done).await;
    }

    /// The history's bug: a cursor crossing two commits asked for both, and
    /// the second ask cancelled the first, which then waited forever.
    #[tokio::test]
    async fn asks_for_two_commits_are_both_answered() {
        let queue = ReviewQueue::default();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let (_other, release) = hold_the_turn(&queue).await;

        let mut client = queue.lane();
        client.submit(ask(1, ReviewBase::Commit, Some("aaaa")), || 1, deliver_to(&done_tx, "a"));
        client.submit(ask(1, ReviewBase::Commit, Some("bbbb")), || 2, deliver_to(&done_tx, "b"));
        release.send(()).unwrap();

        let mut delivered = [within(&mut done).await, within(&mut done).await];
        delivered.sort();
        assert_eq!(delivered, [Some(("a", 1)), Some(("b", 2))]);
    }

    #[tokio::test]
    async fn asks_for_one_side_of_two_checkouts_are_both_answered() {
        let queue = ReviewQueue::default();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let (_other, release) = hold_the_turn(&queue).await;

        let mut client = queue.lane();
        client.submit(ask(1, ReviewBase::Unstaged, None), || 1, deliver_to(&done_tx, "a"));
        client.submit(ask(2, ReviewBase::Unstaged, None), || 2, deliver_to(&done_tx, "b"));
        release.send(()).unwrap();

        let mut delivered = [within(&mut done).await, within(&mut done).await];
        delivered.sort();
        assert_eq!(delivered, [Some(("a", 1)), Some(("b", 2))]);
    }

    #[tokio::test]
    async fn a_generation_that_panics_answers_every_ask_it_holds_with_why() {
        let queue = ReviewQueue::default();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let (_other, release) = hold_the_turn(&queue).await;

        let mut client = queue.lane::<u32>();
        for _ in 0..2 {
            let done_tx = done_tx.clone();
            client.submit(
                ask(1, ReviewBase::Unstaged, None),
                || panic!("the diff could not be read"),
                move |result| {
                    let _ = done_tx.send(result);
                },
            );
        }
        release.send(()).unwrap();

        assert!(matches!(within(&mut done).await, Some(Err(_))));
        assert!(matches!(within(&mut done).await, Some(Err(_))));
    }

    /// A history crossing many commits must not put every one of them
    /// ahead of another client's review.
    #[tokio::test]
    async fn a_client_with_many_asks_holds_one_place_in_the_turn() {
        let queue = ReviewQueue::default();
        let (started_tx, mut started) = mpsc::unbounded_channel();
        let (_other, release) = hold_the_turn(&queue).await;

        let mut a = queue.lane::<u32>();
        for (n, commit) in [(1, "aaaa"), (2, "bbbb"), (3, "cccc")] {
            a.submit(ask(1, ReviewBase::Commit, Some(commit)), said(&started_tx, n), |_| {});
        }
        tokio::task::yield_now().await;
        let mut b = queue.lane::<u32>();
        b.submit(ask(1, ReviewBase::Unstaged, None), said(&started_tx, 9), |_| {});
        tokio::task::yield_now().await;
        release.send(()).unwrap();

        let mut order = Vec::new();
        for _ in 0..4 {
            order.push(within(&mut started).await);
        }
        assert_eq!(order, [Some(1), Some(9), Some(2), Some(3)]);
    }

    /// The review asks for the side on screen first, and paints it first.
    #[tokio::test]
    async fn a_lane_reads_its_asks_in_the_order_asked() {
        let queue = ReviewQueue::default();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let (_other, release) = hold_the_turn(&queue).await;

        let mut client = queue.lane();
        client.submit(ask(1, ReviewBase::Unstaged, None), || 1, deliver_to(&done_tx, "a"));
        client.submit(ask(1, ReviewBase::Staged, None), || 2, deliver_to(&done_tx, "b"));
        release.send(()).unwrap();

        assert_eq!(within(&mut done).await, Some(("a", 1)));
        assert_eq!(within(&mut done).await, Some(("b", 2)));
    }
}
