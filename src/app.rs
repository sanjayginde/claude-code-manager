use crate::data::{Project, Session};
use ratatui::widgets::ListState;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pane {
    Projects,
    Sessions,
}

pub struct App {
    pub projects: Vec<Project>,
    pub selected_project: usize,
    pub selected_session: usize,
    pub active_pane: Pane,
    pub show_delete_confirm: bool,
    pub status: String,
    pub projects_state: ListState,
    pub sessions_state: ListState,
}

impl App {
    pub fn new(projects: Vec<Project>) -> Self {
        let mut projects_state = ListState::default();
        let mut sessions_state = ListState::default();

        if !projects.is_empty() {
            projects_state.select(Some(0));
            if !projects[0].sessions.is_empty() {
                sessions_state.select(Some(0));
            }
        }

        Self {
            projects,
            selected_project: 0,
            selected_session: 0,
            active_pane: Pane::Projects,
            show_delete_confirm: false,
            status: String::new(),
            projects_state,
            sessions_state,
        }
    }

    pub fn current_sessions(&self) -> &[Session] {
        self.projects
            .get(self.selected_project)
            .map(|p| p.sessions.as_slice())
            .unwrap_or(&[])
    }

    pub fn current_session(&self) -> Option<&Session> {
        self.current_sessions().get(self.selected_session)
    }

    pub fn update_title(&mut self, uuid: &str, title: String) {
        for p in &mut self.projects {
            for s in &mut p.sessions {
                if s.uuid == uuid {
                    s.title = Some(title);
                    return;
                }
            }
        }
    }

    pub fn nav_up(&mut self) {
        match self.active_pane {
            Pane::Projects => {
                if self.selected_project > 0 {
                    self.selected_project -= 1;
                    self.selected_session = 0;
                    self.sync_states();
                }
            }
            Pane::Sessions => {
                if self.selected_session > 0 {
                    self.selected_session -= 1;
                    self.sessions_state.select(Some(self.selected_session));
                }
            }
        }
    }

    pub fn nav_down(&mut self) {
        match self.active_pane {
            Pane::Projects => {
                if self.selected_project + 1 < self.projects.len() {
                    self.selected_project += 1;
                    self.selected_session = 0;
                    self.sync_states();
                }
            }
            Pane::Sessions => {
                let n = self.current_sessions().len();
                if n > 0 && self.selected_session + 1 < n {
                    self.selected_session += 1;
                    self.sessions_state.select(Some(self.selected_session));
                }
            }
        }
    }

    pub fn switch_pane(&mut self) {
        self.active_pane = match self.active_pane {
            Pane::Projects => Pane::Sessions,
            Pane::Sessions => Pane::Projects,
        };
    }

    pub fn delete_current(&mut self) -> anyhow::Result<()> {
        let pi = self.selected_project;
        let si = self.selected_session;

        if let Some(proj) = self.projects.get(pi) {
            if let Some(sess) = proj.sessions.get(si) {
                if sess.jsonl_path.exists() {
                    std::fs::remove_file(&sess.jsonl_path)?;
                }
                if let Some(parent) = sess.jsonl_path.parent() {
                    let uuid_dir = parent.join(&sess.uuid);
                    if uuid_dir.is_dir() {
                        std::fs::remove_dir_all(&uuid_dir)?;
                    }
                }
                let tp = sess.title_cache_path();
                if tp.exists() {
                    let _ = std::fs::remove_file(tp);
                }
            }
        }

        if let Some(proj) = self.projects.get_mut(pi) {
            if si < proj.sessions.len() {
                proj.sessions.remove(si);
            }
        }

        if self.projects.get(pi).map_or(false, |p| p.sessions.is_empty()) {
            self.projects.remove(pi);
            self.selected_project = if self.projects.is_empty() {
                0
            } else {
                pi.min(self.projects.len() - 1)
            };
            self.selected_session = 0;
        } else if let Some(proj) = self.projects.get(pi) {
            if !proj.sessions.is_empty() {
                self.selected_session = si.min(proj.sessions.len() - 1);
            }
        }

        self.sync_states();
        Ok(())
    }

    fn sync_states(&mut self) {
        let proj_sel = if self.projects.is_empty() {
            None
        } else {
            Some(self.selected_project.min(self.projects.len() - 1))
        };
        let sess_sel = if self.current_sessions().is_empty() {
            None
        } else {
            Some(self.selected_session)
        };
        self.projects_state.select(proj_sel);
        self.sessions_state.select(sess_sel);
    }
}
