mod app;
mod data;
mod session_store;
mod title_service;
mod titles;
mod ui;

use std::sync::Arc;

use app::{Action, App, Response};
use session_store::FsSessionStore;
use title_service::{AnthropicTitleService, TitleHandle, TitleService};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::path::PathBuf;
use std::time::Duration;

enum Outcome {
    Quit,
    Resume { cwd: PathBuf, uuid: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let store: session_store::DynStore = Arc::new(FsSessionStore::new()?);
    let projects = store.load()?;

    let all_sessions: Vec<&data::Session> =
        projects.iter().flat_map(|p| p.sessions.iter()).collect();
    let title_handle = AnthropicTitleService { store: Arc::clone(&store) }.start(&all_sessions);

    let app = App::new(projects, store);
    let outcome = run_tui(app, title_handle)?;

    if let Outcome::Resume { cwd, uuid } = outcome {
        if cwd.as_os_str().is_empty() || !cwd.exists() {
            if !cwd.as_os_str().is_empty() {
                eprintln!(
                    "Warning: original project path {:?} no longer exists; resuming from current dir",
                    cwd
                );
            }
        } else {
            std::env::set_current_dir(&cwd)?;
        }

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = std::process::Command::new("claude")
                .args(["--resume", &uuid])
                .exec();
            return Err(anyhow::anyhow!("failed to exec claude: {}", err));
        }

        #[cfg(not(unix))]
        {
            let status = std::process::Command::new("claude")
                .args(["--resume", &uuid])
                .status()?;
            std::process::exit(status.code().unwrap_or(0));
        }
    }

    Ok(())
}

fn run_tui(mut app: App, handle: TitleHandle) -> anyhow::Result<Outcome> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let outcome = tui_loop(&mut terminal, &mut app, &handle.rx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    handle.cancel(); // abort any in-flight title generation tasks

    outcome
}

fn tui_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    rx: &std::sync::mpsc::Receiver<(String, String)>,
) -> anyhow::Result<Outcome>
where
    B::Error: Send + Sync + 'static,
{
    loop {
        while let Ok((uuid, title)) = rx.try_recv() {
            app.dispatch(Action::TitleUpdate { uuid, title })?;
        }

        terminal.draw(|f| ui::render(f, app))?;

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };

        if key.kind == event::KeyEventKind::Release {
            continue;
        }

        let action = if app.editing_title().is_some() {
            // Title edit mode: intercept all keys for the input buffer.
            match key.code {
                KeyCode::Esc => Action::CancelEditTitle,
                KeyCode::Enter => Action::ConfirmEditTitle,
                KeyCode::Backspace => Action::EditTitleBackspace,
                KeyCode::Char(c) => Action::EditTitleChar(c),
                _ => continue,
            }
        } else if app.delete_pending() {
            // Confirmation overlay: only these keys are meaningful.
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => Action::ConfirmDelete,
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Action::CancelDelete,
                _ => continue,
            }
        } else {
            match key.code {
                KeyCode::Char('q') => Action::Quit,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
                KeyCode::Up | KeyCode::Char('k') => Action::NavUp,
                KeyCode::Down | KeyCode::Char('j') => Action::NavDown,
                KeyCode::Tab
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Char('h')
                | KeyCode::Char('l') => Action::SwitchPane,
                KeyCode::Enter => Action::Resume,
                KeyCode::Char('d') => Action::RequestDelete,
                KeyCode::Char('e') => Action::StartEditTitle,
                KeyCode::Char('y') => Action::CopyMessage,
                _ => continue,
            }
        };

        match app.dispatch(action)? {
            Response::Continue => {}
            Response::Quit => return Ok(Outcome::Quit),
            Response::ResumeSession { cwd, uuid } => return Ok(Outcome::Resume { cwd, uuid }),
        }
    }
}
