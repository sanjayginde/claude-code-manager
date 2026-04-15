use std::path::PathBuf;

use arboard::Clipboard;
use ratatui::widgets::ListState;

use crate::data::{Project, Session, SessionTitle};
use crate::session_store::DynStore;

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

/// The active input mode of the application.
///
/// `tui_loop` dispatches key events based solely on this value — no knowledge
/// of individual `App` fields required. Adding a new modal means adding a
/// variant here and a routing arm in `tui_loop`, with the compiler enforcing
/// that both are handled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Modal {
    None,
    EditTitle,
    ConfirmDelete,
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
    StartEditTitle,
    EditTitleChar(char),
    EditTitleBackspace,
    ConfirmEditTitle,
    CancelEditTitle,
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

pub struct App {
    projects: Vec<Project>,
    selection: Selection,
    active_pane: Pane,
    delete_pending: bool,
    editing_title: Option<String>,
    status: String,
    store: DynStore,
}

impl App {
    pub fn new(projects: Vec<Project>, store: DynStore, cwd: Option<PathBuf>) -> Self {
        let initial_project = cwd
            .and_then(|cwd| {
                projects.iter().position(|p| {
                    p.sessions
                        .first()
                        .map(|s| cwd.starts_with(&s.cwd))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(0);
        Self {
            projects,
            selection: Selection { project: initial_project, session: 0 },
            active_pane: Pane::Projects,
            delete_pending: false,
            editing_title: None,
            status: String::new(),
            store,
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
                // Don't overwrite the title of the session currently being edited —
                // the user's in-flight buffer is the authoritative state right now.
                let being_edited = self.editing_title.is_some()
                    && self.current_session().is_some_and(|s| s.uuid == uuid);
                if !being_edited {
                    for p in &mut self.projects {
                        for s in &mut p.sessions {
                            if s.uuid == uuid {
                                s.title = SessionTitle::Loaded(title);
                                return Ok(Response::Continue);
                            }
                        }
                    }
                }
                Ok(Response::Continue)
            }
            Action::StartEditTitle => {
                if self.active_pane == Pane::Sessions && self.current_session().is_some() {
                    let prefill = match self.current_session().map(|s| &s.title) {
                        Some(SessionTitle::Loaded(t)) => t.clone(),
                        _ => String::new(),
                    };
                    self.editing_title = Some(prefill);
                }
                Ok(Response::Continue)
            }
            Action::EditTitleChar(c) => {
                if let Some(buf) = &mut self.editing_title {
                    buf.push(c);
                }
                Ok(Response::Continue)
            }
            Action::EditTitleBackspace => {
                if let Some(buf) = &mut self.editing_title {
                    buf.pop();
                }
                Ok(Response::Continue)
            }
            Action::CancelEditTitle => {
                self.editing_title = None;
                Ok(Response::Continue)
            }
            Action::ConfirmEditTitle => {
                if let Some(buf) = self.editing_title.take() {
                    let trimmed = buf.trim().to_string();
                    if !trimmed.is_empty() {
                        let pi = self.selection.project;
                        let si = self.selection.session;
                        if let Some(sess) = self.projects.get_mut(pi).and_then(|p| p.sessions.get_mut(si)) {
                            self.store.save_title(sess, &trimmed)?;
                            sess.title = SessionTitle::Loaded(trimmed);
                            self.status = "Title updated.".into();
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

    /// The active input mode. `tui_loop` routes key events based solely on this.
    pub fn modal(&self) -> Modal {
        if self.editing_title.is_some() {
            Modal::EditTitle
        } else if self.delete_pending {
            Modal::ConfirmDelete
        } else {
            Modal::None
        }
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

    pub fn editing_title(&self) -> Option<&str> {
        self.editing_title.as_deref()
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

        // Disk first — any error here is a clean abort; in-memory state is
        // not touched until this succeeds.
        if let Some(sess) = self.projects.get(pi).and_then(|p| p.sessions.get(si)) {
            self.store.delete(sess)?;
        }

        // In-memory Vec mutation (infallible after disk op succeeds).
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
    use std::sync::Arc;
    use std::time::SystemTime;

    use super::*;
    use crate::data::{Session, SessionTitle};
    use crate::session_store::NullSessionStore;

    fn make_session(uuid: &str) -> Session {
        Session {
            uuid: uuid.to_string(),
            jsonl_path: PathBuf::from(format!("/tmp/{uuid}.jsonl")),
            cwd: PathBuf::from("/tmp"),
            git_branch: None,
            first_message: Some("hello".into()),
            title: SessionTitle::Absent,
            last_modified: SystemTime::UNIX_EPOCH,
            size_bytes: 0,
            parse_error: None,
        }
    }

    fn make_app(project_session_counts: &[usize]) -> App {
        let projects = project_session_counts
            .iter()
            .enumerate()
            .map(|(pi, &n)| Project {
                label: format!("proj{pi}"),
                sessions: (0..n).map(|si| make_session(&format!("p{pi}s{si}"))).collect(),
            })
            .collect();
        App::new(projects, Arc::new(NullSessionStore), None)
    }

    fn make_session_with_cwd(uuid: &str, cwd: &str) -> Session {
        Session {
            uuid: uuid.to_string(),
            jsonl_path: PathBuf::from(format!("/tmp/{uuid}.jsonl")),
            cwd: PathBuf::from(cwd),
            git_branch: None,
            first_message: Some("hello".into()),
            title: SessionTitle::Absent,
            last_modified: SystemTime::UNIX_EPOCH,
            size_bytes: 0,
            parse_error: None,
        }
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

    #[test]
    fn start_edit_title_prefills_existing_title() {
        let mut app = make_app(&[2]);
        app.projects[0].sessions[0].title = SessionTitle::Loaded("Existing Title".into());
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::StartEditTitle).unwrap();
        assert_eq!(app.editing_title(), Some("Existing Title"));
    }

    #[test]
    fn start_edit_title_empty_when_no_title() {
        let mut app = make_app(&[2]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::StartEditTitle).unwrap();
        assert_eq!(app.editing_title(), Some(""));
    }

    #[test]
    fn start_edit_title_noop_in_projects_pane() {
        let mut app = make_app(&[2]);
        // active_pane starts as Projects
        app.dispatch(Action::StartEditTitle).unwrap();
        assert_eq!(app.editing_title(), None);
    }

    #[test]
    fn cancel_edit_title_clears_buffer() {
        let mut app = make_app(&[2]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::StartEditTitle).unwrap();
        app.dispatch(Action::EditTitleChar('H')).unwrap();
        app.dispatch(Action::CancelEditTitle).unwrap();
        assert_eq!(app.editing_title(), None);
        assert_eq!(app.current_session().unwrap().title, SessionTitle::Absent); // title unchanged
    }

    #[test]
    fn confirm_edit_title_updates_session_and_clears_buffer() {
        let mut app = make_app(&[2]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::StartEditTitle).unwrap();
        for c in "New Title".chars() {
            app.dispatch(Action::EditTitleChar(c)).unwrap();
        }
        app.dispatch(Action::ConfirmEditTitle).unwrap();
        assert_eq!(app.editing_title(), None);
        assert_eq!(app.current_session().unwrap().title, SessionTitle::Loaded("New Title".into()));
    }

    #[test]
    fn confirm_edit_title_ignores_whitespace_only_input() {
        let mut app = make_app(&[2]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::StartEditTitle).unwrap();
        app.dispatch(Action::EditTitleChar(' ')).unwrap();
        app.dispatch(Action::ConfirmEditTitle).unwrap();
        assert_eq!(app.editing_title(), None);
        assert_eq!(app.current_session().unwrap().title, SessionTitle::Absent); // not updated
    }

    #[test]
    fn edit_title_backspace_removes_last_char() {
        let mut app = make_app(&[1]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::StartEditTitle).unwrap();
        app.dispatch(Action::EditTitleChar('H')).unwrap();
        app.dispatch(Action::EditTitleChar('i')).unwrap();
        app.dispatch(Action::EditTitleBackspace).unwrap();
        assert_eq!(app.editing_title(), Some("H"));
    }

    #[test]
    fn title_update_does_not_clobber_session_being_edited() {
        let mut app = make_app(&[2]);
        let uuid = app.projects[0].sessions[0].uuid.clone();
        app.dispatch(Action::SwitchPane).unwrap();
        // Start editing the first session
        app.dispatch(Action::StartEditTitle).unwrap();
        app.dispatch(Action::EditTitleChar('M')).unwrap();
        // Async title arrives for the same session while editing
        app.dispatch(Action::TitleUpdate { uuid: uuid.clone(), title: "Async Title".into() }).unwrap();
        // Edit buffer is intact, session.title was NOT overwritten
        assert_eq!(app.editing_title(), Some("M"));
        assert_eq!(app.current_session().unwrap().title, SessionTitle::Absent);
    }

    #[test]
    fn title_update_applies_normally_for_other_sessions() {
        let mut app = make_app(&[2]);
        let other_uuid = app.projects[0].sessions[1].uuid.clone();
        app.dispatch(Action::SwitchPane).unwrap();
        // Start editing session 0
        app.dispatch(Action::StartEditTitle).unwrap();
        // Title arrives for session 1 (not being edited)
        app.dispatch(Action::TitleUpdate { uuid: other_uuid.clone(), title: "Other Title".into() }).unwrap();
        // Session 1 title was updated normally
        assert_eq!(app.projects[0].sessions[1].title, SessionTitle::Loaded("Other Title".into()));
    }

    // ── initial cwd focus ─────────────────────────────────────────────────────

    #[test]
    fn initial_cwd_selects_matching_project() {
        let projects = vec![
            Project {
                label: "proj0".into(),
                sessions: vec![make_session_with_cwd("s0", "/home/user/alpha")],
            },
            Project {
                label: "proj1".into(),
                sessions: vec![make_session_with_cwd("s1", "/home/user/beta")],
            },
        ];
        let app = App::new(projects, Arc::new(NullSessionStore), Some(PathBuf::from("/home/user/beta")));
        assert_eq!(app.projects_list_state().selected(), Some(1));
    }

    #[test]
    fn initial_cwd_matches_subdirectory_of_project_root() {
        let projects = vec![
            Project {
                label: "proj0".into(),
                sessions: vec![make_session_with_cwd("s0", "/home/user/alpha")],
            },
            Project {
                label: "proj1".into(),
                sessions: vec![make_session_with_cwd("s1", "/home/user/beta")],
            },
        ];
        let app = App::new(projects, Arc::new(NullSessionStore), Some(PathBuf::from("/home/user/beta/src")));
        assert_eq!(app.projects_list_state().selected(), Some(1));
    }

    #[test]
    fn initial_cwd_no_match_falls_back_to_zero() {
        let projects = vec![
            Project {
                label: "proj0".into(),
                sessions: vec![make_session_with_cwd("s0", "/home/user/alpha")],
            },
        ];
        let app = App::new(projects, Arc::new(NullSessionStore), Some(PathBuf::from("/home/user/other")));
        assert_eq!(app.projects_list_state().selected(), Some(0));
    }

    #[test]
    fn initial_cwd_none_falls_back_to_zero() {
        let app = make_app(&[2]);
        assert_eq!(app.projects_list_state().selected(), Some(0));
    }

    // ── modal() ───────────────────────────────────────────────────────────────

    #[test]
    fn modal_is_none_by_default() {
        let app = make_app(&[1]);
        assert_eq!(app.modal(), Modal::None);
    }

    #[test]
    fn modal_is_edit_title_while_editing() {
        let mut app = make_app(&[1]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::StartEditTitle).unwrap();
        assert_eq!(app.modal(), Modal::EditTitle);
        // cancelling returns to None
        app.dispatch(Action::CancelEditTitle).unwrap();
        assert_eq!(app.modal(), Modal::None);
    }

    #[test]
    fn modal_is_confirm_delete_while_pending() {
        let mut app = make_app(&[1]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::RequestDelete).unwrap();
        assert_eq!(app.modal(), Modal::ConfirmDelete);
        // cancelling returns to None
        app.dispatch(Action::CancelDelete).unwrap();
        assert_eq!(app.modal(), Modal::None);
    }

    #[test]
    fn modal_is_none_after_delete_confirmed() {
        let mut app = make_app(&[2]);
        app.dispatch(Action::SwitchPane).unwrap();
        app.dispatch(Action::RequestDelete).unwrap();
        assert_eq!(app.modal(), Modal::ConfirmDelete);
        app.dispatch(Action::ConfirmDelete).unwrap();
        assert_eq!(app.modal(), Modal::None);
    }
}
