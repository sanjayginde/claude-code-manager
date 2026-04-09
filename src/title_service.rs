use std::sync::{mpsc, Arc};

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::data::Session;
use crate::session_store::DynStore;

/// Maximum number of title generation tasks that may run concurrently.
/// Prevents bursting 50+ simultaneous API calls when many sessions need titles.
const MAX_CONCURRENT: usize = 6;

/// Handle to in-flight title generation tasks.
///
/// Owns the receiver end of the title update channel and the spawned task set.
/// Dropping this handle (or calling [`cancel`](TitleHandle::cancel)) aborts all
/// in-flight tasks immediately.
pub struct TitleHandle {
    /// Receive `(uuid, title)` pairs as tasks complete. Poll this in the TUI loop.
    pub rx: mpsc::Receiver<(String, String)>,
    /// Dropping the JoinSet aborts all spawned tasks.
    _tasks: JoinSet<()>,
}

impl TitleHandle {
    /// Abort all in-flight title generation tasks.
    pub fn cancel(self) {
        // Dropping self drops _tasks, which aborts all tasks via JoinSet's Drop impl.
    }
}

/// Generates and delivers session titles asynchronously.
///
/// Implementations spawn background work and deliver `(uuid, title)` pairs
/// via the receiver returned by [`start`](TitleService::start).
pub trait TitleService {
    fn start(&self, sessions: &[&Session]) -> TitleHandle;
}

/// Production implementation: calls the Anthropic API and caches results via
/// the `SessionStore`. Limits concurrency to [`MAX_CONCURRENT`] simultaneous
/// requests; save errors are logged rather than silently discarded.
pub struct AnthropicTitleService {
    pub store: DynStore,
}

impl TitleService for AnthropicTitleService {
    fn start(&self, sessions: &[&Session]) -> TitleHandle {
        let (tx, rx) = mpsc::channel::<(String, String)>();
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
        let mut tasks = JoinSet::new();

        for sess in sessions {
            if !sess.needs_title() {
                continue;
            }
            let tx = tx.clone();
            let store = Arc::clone(&self.store);
            let uuid = sess.uuid.clone();
            let msg = sess.first_message.clone().unwrap();
            let sess_clone = (*sess).clone();
            let sem = Arc::clone(&semaphore);

            tasks.spawn(async move {
                // Acquire a permit before making the API call; released when
                // _permit is dropped at the end of this task.
                let _permit = sem.acquire().await.unwrap();
                if let Some(title) = crate::titles::generate_title(&msg).await {
                    if let Err(e) = store.save_title(&sess_clone, &title) {
                        eprintln!(
                            "warning: failed to save title for session {}: {e}",
                            &uuid[..8.min(uuid.len())]
                        );
                    }
                    let _ = tx.send((uuid, title));
                }
            });
        }

        TitleHandle { rx, _tasks: tasks }
    }
}

/// No-op implementation for tests — sends nothing, spawns nothing.
#[cfg(test)]
#[allow(dead_code)]
pub struct NoopTitleService;

#[cfg(test)]
impl TitleService for NoopTitleService {
    fn start(&self, _sessions: &[&Session]) -> TitleHandle {
        let (_tx, rx) = mpsc::channel();
        TitleHandle { rx, _tasks: JoinSet::new() }
    }
}
