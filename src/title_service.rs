use std::sync::mpsc;

use crate::data::Session;
use crate::session_store::DynStore;

/// Generates and delivers session titles asynchronously.
///
/// Implementations spawn background work and send `(uuid, title)` pairs
/// over `tx` as results arrive. The caller drives the TUI loop; titles
/// appear incrementally as the channel drains.
pub trait TitleService {
    fn start(&self, sessions: &[&Session], tx: mpsc::Sender<(String, String)>);
}

/// Production implementation: calls the Anthropic API and caches results via
/// the `SessionStore`, so no direct filesystem access leaks out of this module.
pub struct AnthropicTitleService {
    pub store: DynStore,
}

impl TitleService for AnthropicTitleService {
    fn start(&self, sessions: &[&Session], tx: mpsc::Sender<(String, String)>) {
        for sess in sessions {
            if !sess.needs_title() {
                continue;
            }
            let tx = tx.clone();
            let store = std::sync::Arc::clone(&self.store);
            let uuid = sess.uuid.clone();
            let msg = sess.first_message.clone().unwrap();
            let sess_clone = (*sess).clone();
            tokio::spawn(async move {
                if let Some(title) = crate::titles::generate_title(&msg).await {
                    let _ = store.save_title(&sess_clone, &title);
                    let _ = tx.send((uuid, title));
                }
            });
        }
    }
}

/// No-op implementation for tests — sends nothing, spawns nothing.
#[cfg(test)]
#[allow(dead_code)]
pub struct NoopTitleService;

#[cfg(test)]
impl TitleService for NoopTitleService {
    fn start(&self, _sessions: &[&Session], _tx: mpsc::Sender<(String, String)>) {}
}
