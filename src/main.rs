mod app;
mod data;
mod titles;
mod ui;

use app::{App, Pane};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

enum Outcome {
    Quit,
    Resume { cwd: PathBuf, uuid: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let projects = data::load_projects()?;

    let (tx, rx) = mpsc::channel::<(String, String)>();

    for proj in &projects {
        for sess in &proj.sessions {
            if sess.needs_title() {
                let tx = tx.clone();
                let uuid = sess.uuid.clone();
                let msg = sess.first_message.clone().unwrap();
                let cache = sess.title_cache_path();
                tokio::spawn(async move {
                    if let Some(title) = titles::generate_title(&msg).await {
                        let _ = std::fs::write(&cache, &title);
                        let _ = tx.send((uuid, title));
                    }
                });
            }
        }
    }

    let app = App::new(projects);
    let outcome = run_tui(app, rx)?;

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

fn run_tui(mut app: App, rx: mpsc::Receiver<(String, String)>) -> anyhow::Result<Outcome> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let outcome = tui_loop(&mut terminal, &mut app, &rx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    outcome
}

fn tui_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    rx: &mpsc::Receiver<(String, String)>,
) -> anyhow::Result<Outcome> {
    loop {
        while let Ok((uuid, title)) = rx.try_recv() {
            app.update_title(&uuid, title);
        }

        terminal.draw(|f| ui::render(f, app))?;

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };

        // Swallow key-release events (crossterm sometimes emits them)
        if key.kind == event::KeyEventKind::Release {
            continue;
        }

        if app.show_delete_confirm {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    app.show_delete_confirm = false;
                    match app.delete_current() {
                        Ok(()) => app.status = "Session deleted.".into(),
                        Err(e) => app.status = format!("Error: {e}"),
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    app.show_delete_confirm = false;
                }
                _ => {}
            }
            continue;
        }

        match key.code {
            KeyCode::Char('q') => return Ok(Outcome::Quit),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Outcome::Quit)
            }

            KeyCode::Up | KeyCode::Char('k') => {
                app.status.clear();
                app.nav_up();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.status.clear();
                app.nav_down();
            }

            KeyCode::Tab
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Char('h')
            | KeyCode::Char('l') => {
                app.switch_pane();
            }

            KeyCode::Enter => {
                if app.active_pane == Pane::Sessions {
                    if let Some(sess) = app.current_session() {
                        return Ok(Outcome::Resume {
                            cwd: sess.cwd.clone(),
                            uuid: sess.uuid.clone(),
                        });
                    }
                } else {
                    app.switch_pane();
                }
            }

            KeyCode::Char('d') => {
                if app.active_pane == Pane::Sessions && app.current_session().is_some() {
                    app.show_delete_confirm = true;
                    app.status.clear();
                }
            }

            _ => {}
        }
    }
}
