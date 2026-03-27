use std::path::PathBuf;

use arboard::Clipboard;
use ratatui::widgets::ListState;

use crate::data::{Project, Session};
use crate::fs::{Filesystem, RealFs};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pane {
    Projects,
    Sessions,
}

impl Pane {
    fn toggle(self) -> Self {
        match self {
            Pane::Projects => Pane::Sessions,
            Pane::Sessions => Pane::Projects,
        }
    }
}

pub enum Action {
    NavUp,
    NavDown,
    SwitchPane,
    RequestDelete,
    ConfirmDelete,
    CancelDelete,
    Resume,
    Quit,
    CopyMessage,
    TitleUpdate { uuid: String, title: String },
}

pub enum Response {
    Continue,
    ResumeSession { cwd: PathBuf, uuid: String },
    Quit,
}

struct Selection {
    project: usize,
    session: usize,
}

pub struct App<F: Filesystem = RealFs> {
    projects: Vec<Project>,
    selection: Selection,
    active_pane: Pane,
    delete_pending: bool,
    status: String,
    fs: F,
}

impl App<RealFs> {
    pub fn new(projects: Vec<Project>) -> Self {
        Self::with_fs(projects, RealFs)
    }
}

impl<F: Filesystem> App<F> {
    pub fn with_fs(projects: Vec<Project>, fs: F) -> Self {
        Self {
            projects,
            selection: Selection { project: 0, session: 0 },
            active_pane: Pane::Projects,
            delete_pending: false,
            status: String::new(),
            fs,
        }
    }

    /// The single mutating entry point. All state changes go through here.
    pub fn dispatch(&mut self, action: Action) -> anyhow::Result<Response> {
        self.status.clear();
        match action {
            Action::NavUp => {
                self.nav_up();
                Ok(Response::Continue)
            }
            Action::NavDown => {
                self.nav_down();
                Ok(Response::Continue)
            }
            Action::SwitchPane => {
                self.active_pane = self.active_pane.toggle();
                Ok(Response::Continue)
            }
            Action::RequestDelete => {
                if self.active_pane == Pane::Sessions && self.current_session().is_some() {
                    self.delete_pending = true;
                }
                Ok(Response::Continue)
            }
            Action::ConfirmDelete => {
                if self.delete_pending {
                    self.delete_pending = false;
                    self.execute_delete()?;
                    self.status = "Session deleted.".into();
                }
                Ok(Response::Continue)
            }
            Action::CancelDelete => {
                self.delete_pending = false;
                Ok(Response::Continue)
            }
            Action::Resume => {
                if self.active_pane == Pane::Sessions {
                    if let Some(sess) = self.current_session() {
                        return Ok(Response::ResumeSession {
                            cwd: sess.cwd.clone(),
                            uuid: sess.uuid.clone(),
                        });
                    }
                } else {
                    self.active_pane = Pane::Sessions;
                }
                Ok(Response::Continue)
            }
            Action::CopyMessage => {
                match self.current_session().and_then(|s| s.first_message.as_deref()) {
                    Some(msg) => {
                        match Clipboard::new().and_then(|mut cb| cb.set_text(msg)) {
                            Ok(()) => self.status = "Message copied.".into(),
                            Err(e) => self.status = format!("Copy failed: {e}"),
                        }
                    }
                    None => self.status = "Nothing to copy.".into(),
                }
                Ok(Response::Continue)
            }
            Action::Quit => Ok(Response::Quit),
            Action::TitleUpdate { uuid, title } => {
                for p in &mut self.projects {
                    for s in &mut p.sessions {
                        if s.uuid == uuid {
                            s.title = Some(title);
                            return Ok(Response::Continue);
                        }
                    }
                }
                Ok(Response::Continue)
            }
        }
    }

    // ── Read-only accessors for the renderer ─────────────────────────────────

    pub fn projects(&self) -> &[Project] {
        &self.projects
    }

    pub fn active_pane(&self) -> Pane {
        self.active_pane
    }

    pub fn delete_pending(&self) -> bool {
        self.delete_pending
    }

    pub fn current_sessions(&self) -> &[Session] {
        self.projects
            .get(self.selection.project)
            .map(|p| p.sessions.as_slice())
            .unwrap_or(&[])
    }

    pub fn current_session(&self) -> Option<&Session> {
        self.current_sessions().get(self.selection.session)
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn current_project_label(&self) -> Option<&str> {
        self.projects.get(self.selection.project).map(|p| p.label.as_str())
    }

    /// Derives a fresh ListState at draw time — never stored, never stale.
    pub fn projects_list_state(&self) -> ListState {
        let mut s = ListState::default();
        if !self.projects.is_empty() {
            s.select(Some(self.selection.project));
        }
        s
    }

    /// Derives a fresh ListState at draw time — never stored, never stale.
    pub fn sessions_list_state(&self) -> ListState {
        let mut s = ListState::default();
        if !self.current_sessions().is_empty() {
            s.select(Some(self.selection.session));
        }
        s
    }

    // ── Private mutation helpers ──────────────────────────────────────────────

    fn nav_up(&mut self) {
        match self.active_pane {
            Pane::Projects => {
                if self.selection.project > 0 {
                    self.selection.project -= 1;
                    self.selection.session = 0;
                }
            }
            Pane::Sessions => {
                if self.selection.session > 0 {
                    self.selection.session -= 1;
                }
            }
        }
    }

    fn nav_down(&mut self) {
        match self.active_pane {
            Pane::Projects => {
                if self.selection.project + 1 < self.projects.len() {
                    self.selection.project += 1;
                    self.selection.session = 0;
                }
            }
            Pane::Sessions => {
                let n = self.current_sessions().len();
                if n > 0 && self.selection.session + 1 < n {
                    self.selection.session += 1;
                }
            }
        }
    }

    fn execute_delete(&mut self) -> anyhow::Result<()> {
        let pi = self.selection.project;
        let si = self.selection.session;

        // Phase 1: filesystem first — any error here is a clean abort;
        // in-memory state is not touched until this succeeds.
        if let Some(sess) = self.projects.get(pi).and_then(|p| p.sessions.get(si)) {
            if self.fs.exists(&sess.jsonl_path) {
                self.fs.remove_file(&sess.jsonl_path)?;
            }
            if let Some(parent) = sess.jsonl_path.parent() {
                let uuid_dir = parent.join(&sess.uuid);
                if self.fs.is_dir(&uuid_dir) {
                    self.fs.remove_dir_all(&uuid_dir)?;
                }
            }
            let title_cache = sess.title_cache_path();
            if self.fs.exists(&title_cache) {
                let _ = self.fs.remove_file(&title_cache);
            }
        }

        // Phase 2: in-memory Vec mutation (infallible after phase 1 succeeds).
        if let Some(proj) = self.projects.get_mut(pi)
            && si < proj.sessions.len() {
                proj.sessions.remove(si);
            }

        // Phase 3: cascade empty-project removal and clamp selection.
        if self.projects.get(pi).is_some_and(|p| p.sessions.is_empty()) {
            self.projects.remove(pi);
            self.selection.project = if self.projects.is_empty() {
                0
            } else {
                pi.min(self.projects.len() - 1)
            };
            self.selection.session = 0;
        } else {
            let sess_len = self.projects.get(pi).map_or(0, |p| p.sessions.len());
            if sess_len > 0 {
                self.selection.session = si.min(sess_len - 1);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::SystemTime;

    use super::*;
    use crate::data::Session;
    use crate::fs::NullFs;

    fn make_session(uuid: &str) -> Session {
        Session {
            uuid: uuid.to_string(),
            jsonl_path: PathBuf::from(format!("/tmp/{uuid}.jsonl")),
            cwd: PathBuf::from("/tmp"),
            git_branch: None,
            first_message: Some("hello".into()),
            title: None,
            last_modified: SystemTime::UNIX_EPOCH,
            size_bytes: 0,
        }
    }

    fn make_app(project_session_counts: &[usize]) -> App<NullFs> {
        let projects = project_session_counts
            .iter()
            .enumerate()
            .map(|(pi, &n)| Project {
                label: format!("proj{pi}"),
                sessions: (0..n).map(|si| make_session(&format!("p{pi}s{si}"))).collect(),
            })
            .collect();
        App::with_fs(projects, NullFs)
    }

    #[test]
    fn nav_down_and_up_in_sessions() {
        let mut app = make_app(&[3]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::NavDown).unwrap();
        app.dispatch(Action::NavDown).unwrap();
        assert_eq!(app.sessions_list_state().selected(), Some(2));
        app.dispatch(Action::NavUp).unwrap();
        assert_eq!(app.sessions_list_state().selected(), Some(1));
    }

    #[test]
    fn nav_does_not_go_out_of_bounds() {
        let mut app = make_app(&[2]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::NavUp).unwrap(); // already at 0
        assert_eq!(app.sessions_list_state().selected(), Some(0));
        app.dispatch(Action::NavDown).unwrap();
        app.dispatch(Action::NavDown).unwrap(); // already at last
        assert_eq!(app.sessions_list_state().selected(), Some(1));
    }

    #[test]
    fn delete_clamps_cursor_to_last_item() {
        let mut app = make_app(&[3]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::NavDown).unwrap();
        app.dispatch(Action::NavDown).unwrap(); // select index 2 (last)
        app.dispatch(Action::RequestDelete).unwrap();
        app.dispatch(Action::ConfirmDelete).unwrap();
        assert_eq!(app.current_sessions().len(), 2);
        assert_eq!(app.sessions_list_state().selected(), Some(1));
    }

    #[test]
    fn delete_last_session_removes_project() {
        let mut app = make_app(&[2, 1]);
        // Navigate to second project
        app.dispatch(Action::NavDown).unwrap();
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::RequestDelete).unwrap();
        app.dispatch(Action::ConfirmDelete).unwrap();
        assert_eq!(app.projects().len(), 1);
        assert_eq!(app.projects_list_state().selected(), Some(0));
    }

    #[test]
    fn request_delete_is_noop_in_projects_pane() {
        let mut app = make_app(&[2]);
        // active_pane starts as Projects
        app.dispatch(Action::RequestDelete).unwrap();
        assert!(!app.delete_pending());
    }

    #[test]
    fn cancel_delete_clears_pending() {
        let mut app = make_app(&[2]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::RequestDelete).unwrap();
        assert!(app.delete_pending());
        app.dispatch(Action::CancelDelete).unwrap();
        assert!(!app.delete_pending());
        assert_eq!(app.current_sessions().len(), 2); // nothing deleted
    }

    #[test]
    fn list_states_are_consistent_after_project_navigation() {
        let mut app = make_app(&[3, 2]);
        app.dispatch(Action::NavDown).unwrap(); // move to proj1
        assert_eq!(app.projects_list_state().selected(), Some(1));
        assert_eq!(app.sessions_list_state().selected(), Some(0)); // reset on project change
        assert_eq!(app.current_sessions().len(), 2);
    }
}
