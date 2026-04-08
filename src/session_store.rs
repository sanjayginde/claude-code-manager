use std::path::PathBuf;
use std::sync::Arc;

use crate::data::{Project, Session};

/// Deep module: owns every disk operation related to sessions.
///
/// All knowledge of the on-disk layout (`.jsonl` files, `.title` sidecars,
/// UUID subdirectories, `~/.claude/projects`) lives behind this trait.
/// No caller needs to know about file paths or deletion ordering.
pub trait SessionStore: Send + Sync + 'static {
    /// Load all projects and sessions from persistent storage, sorted by recency.
    fn load(&self) -> anyhow::Result<Vec<Project>>;

    /// Persist a generated title to the `.title` cache file alongside the session's jsonl.
    fn save_title(&self, session: &Session, title: &str) -> anyhow::Result<()>;

    /// Delete a session and all associated artefacts (jsonl, uuid subdir, title cache).
    /// Idempotent — returns `Ok(())` if files are already absent.
    fn delete(&self, session: &Session) -> anyhow::Result<()>;
}

/// Convenience alias used throughout the codebase.
pub type DynStore = Arc<dyn SessionStore>;

// ── Production implementation ─────────────────────────────────────────────────

/// Filesystem-backed implementation that reads/writes `~/.claude/projects`.
pub struct FsSessionStore {
    base: PathBuf,
}

impl FsSessionStore {
    /// Constructs a store rooted at `~/.claude/projects`.
    pub fn new() -> anyhow::Result<Self> {
        let base = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("no home dir"))?
            .join(".claude/projects");
        Ok(Self { base })
    }

    /// Constructs a store rooted at an arbitrary path — useful for integration
    /// tests that point at a temporary directory.
    #[allow(dead_code)]
    pub fn with_base(base: PathBuf) -> Self {
        Self { base }
    }
}

impl SessionStore for FsSessionStore {
    fn load(&self) -> anyhow::Result<Vec<Project>> {
        crate::data::load_projects_from(&self.base)
    }

    fn save_title(&self, session: &Session, title: &str) -> anyhow::Result<()> {
        std::fs::write(session.title_cache_path(), title)?;
        Ok(())
    }

    fn delete(&self, session: &Session) -> anyhow::Result<()> {
        // Phase 1 (critical): remove the session log.
        if session.jsonl_path.exists() {
            std::fs::remove_file(&session.jsonl_path)?;
        }

        // Phase 2 (best-effort): remove the UUID subdirectory if present.
        if let Some(parent) = session.jsonl_path.parent() {
            let uuid_dir = parent.join(&session.uuid);
            if uuid_dir.is_dir() {
                std::fs::remove_dir_all(&uuid_dir)?;
            }
        }

        // Phase 3 (best-effort): remove the title cache; failure is non-fatal.
        let title_cache = session.title_cache_path();
        if title_cache.exists() {
            let _ = std::fs::remove_file(&title_cache);
        }

        Ok(())
    }
}

// ── Test double ───────────────────────────────────────────────────────────────

/// No-op store for unit tests — all operations succeed silently without
/// touching the filesystem.
#[cfg(test)]
pub struct NullSessionStore;

#[cfg(test)]
impl SessionStore for NullSessionStore {
    fn load(&self) -> anyhow::Result<Vec<Project>> {
        Ok(vec![])
    }
    fn save_title(&self, _session: &Session, _title: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn delete(&self, _session: &Session) -> anyhow::Result<()> {
        Ok(())
    }
}
