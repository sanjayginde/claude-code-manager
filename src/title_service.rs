use std::sync::mpsc;

use crate::data::Session;

/// Generates and delivers session titles asynchronously.
///
/// Implementations spawn background work and send `(uuid, title)` pairs
/// over `tx` as results arrive. The caller drives the TUI loop; titles
/// appear incrementally as the channel drains.
pub trait TitleService {
    fn start(&self, sessions: &[&Session], tx: mpsc::Sender<(String, String)>);
}

/// Production implementation: calls the Anthropic API and caches results.
pub struct AnthropicTitleService;

impl TitleService for AnthropicTitleService {
    fn start(&self, sessions: &[&Session], tx: mpsc::Sender<(String, String)>) {
        for sess in sessions {
            if !sess.needs_title() {
                continue;
            }
            let tx = tx.clone();
            let uuid = sess.uuid.clone();
            let msg = sess.first_message.clone().unwrap();
            let cache = sess.title_cache_path();
            tokio::spawn(async move {
                if let Some(title) = crate::titles::generate_title(&msg).await {
                    let _ = std::fs::write(&cache, &title);
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
