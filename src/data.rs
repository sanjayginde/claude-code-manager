use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct Session {
    pub uuid: String,
    pub jsonl_path: PathBuf,
    pub cwd: PathBuf,
    pub git_branch: Option<String>,
    pub first_message: Option<String>,
    pub title: Option<String>,
    pub last_modified: SystemTime,
    pub size_bytes: u64,
}

impl Session {
    pub fn display_title(&self) -> String {
        if let Some(t) = &self.title {
            return t.clone();
        }
        if let Some(msg) = &self.first_message {
            let s: String = msg.chars().take(70).collect();
            if msg.chars().count() > 70 {
                return format!("{}…", s);
            }
            return s;
        }
        format!("[{}]", &self.uuid[..8.min(self.uuid.len())])
    }

    pub fn age_display(&self) -> String {
        let secs = SystemTime::now()
            .duration_since(self.last_modified)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if secs < 60 {
            return "just now".into();
        }
        let m = secs / 60;
        if m < 60 {
            return format!("{m}m ago");
        }
        let h = m / 60;
        if h < 24 {
            return format!("{h}h ago");
        }
        let d = h / 24;
        if d < 7 {
            return format!("{d}d ago");
        }
        let w = d / 7;
        if w < 5 {
            return format!("{w}w ago");
        }
        format!("{}mo ago", d / 30)
    }

    pub fn size_display(&self) -> String {
        let b = self.size_bytes;
        if b < 1_024 {
            format!("{b}B")
        } else if b < 1_024 * 1_024 {
            format!("{:.0}KB", b as f64 / 1_024.0)
        } else {
            format!("{:.1}MB", b as f64 / (1_024.0 * 1_024.0))
        }
    }

    pub fn title_cache_path(&self) -> PathBuf {
        self.jsonl_path.with_extension("title")
    }

    pub fn needs_title(&self) -> bool {
        self.title.is_none() && self.first_message.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub label: String,
    pub sessions: Vec<Session>,
}

pub fn load_projects() -> anyhow::Result<Vec<Project>> {
    let base = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("no home dir"))?
        .join(".claude/projects");

    if !base.exists() {
        return Ok(vec![]);
    }

    let mut projects = Vec::new();

    for entry in std::fs::read_dir(&base)? {
        let entry = entry?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }

        let sessions = load_sessions(&dir)?;
        if sessions.is_empty() {
            continue;
        }

        let label = sessions
            .first()
            .map(|s| last_two(&s.cwd))
            .unwrap_or_else(|| {
                decode_label(dir.file_name().unwrap_or_default().to_str().unwrap_or(""))
            });

        projects.push(Project { label, sessions });
    }

    projects.sort_by(|a, b| {
        let ta = a
            .sessions
            .first()
            .map(|s| s.last_modified)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let tb = b
            .sessions
            .first()
            .map(|s| s.last_modified)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        tb.cmp(&ta)
    });

    Ok(projects)
}

fn load_sessions(dir: &Path) -> anyhow::Result<Vec<Session>> {
    let mut sessions = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }

        let uuid = match path.file_stem().and_then(|s| s.to_str()) {
            Some(u) if !u.is_empty() => u.to_string(),
            _ => continue,
        };

        let meta = std::fs::metadata(&path)?;
        let last_modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let size_bytes = meta.len();

        let (cwd, git_branch, first_message) = parse_header(&path).unwrap_or_default();

        let title = {
            let cp = path.with_extension("title");
            if cp.exists() {
                std::fs::read_to_string(&cp)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        };

        sessions.push(Session {
            uuid,
            jsonl_path: path,
            cwd: PathBuf::from(cwd.unwrap_or_default()),
            git_branch,
            first_message,
            title,
            last_modified,
            size_bytes,
        });
    }

    sessions.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
    Ok(sessions)
}

fn parse_header(
    path: &Path,
) -> anyhow::Result<(Option<String>, Option<String>, Option<String>)> {
    use std::io::{BufRead, BufReader};

    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);

    let mut cwd = None;
    let mut branch = None;
    let mut first_msg = None;
    let mut n = 0usize;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        n += 1;
        if n > 150 {
            break;
        }

        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        if cwd.is_none() {
            cwd = v["cwd"].as_str().map(String::from);
        }
        if branch.is_none() {
            branch = v["gitBranch"].as_str().map(String::from);
        }

        if first_msg.is_none()
            && v["type"].as_str() == Some("user")
            && v["message"]["role"].as_str() == Some("user")
        {
            let content = &v["message"]["content"];
            if let Some(s) = content.as_str() {
                first_msg = Some(s.trim().to_string());
            } else if let Some(arr) = content.as_array() {
                for item in arr {
                    if item["type"].as_str() == Some("text") {
                        if let Some(s) = item["text"].as_str() {
                            first_msg = Some(s.trim().to_string());
                            break;
                        }
                    }
                }
            }
        }

        if cwd.is_some() && branch.is_some() && first_msg.is_some() {
            break;
        }
    }

    Ok((cwd, branch, first_msg))
}

fn last_two(path: &Path) -> String {
    let parts: Vec<_> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    match parts.len() {
        0 => "/".into(),
        1 => parts[0].into(),
        _ => format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]),
    }
}

fn decode_label(encoded: &str) -> String {
    // Encoded path: -Users-sanjay-work-axio-OneRepo → /Users/sanjay/work/axio/OneRepo
    let decoded = encoded.replace('-', "/");
    let path = Path::new(&decoded);
    last_two(path)
}
