use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Deserialize;

// ── Typed schema for Claude's JSONL log format ────────────────────────────────

/// A single line in a Claude session JSONL file.
/// Unknown fields are ignored; unknown entry types deserialize fine because
/// all fields except `kind` are optional.
#[derive(Deserialize)]
struct LogEntry {
    #[serde(default)]
    cwd: Option<String>,
    #[serde(rename = "gitBranch", default)]
    git_branch: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    message: Option<LogMessage>,
}

#[derive(Deserialize)]
struct LogMessage {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<MessageContent>,
}

/// Claude content can be a plain string or an array of typed blocks.
#[derive(Deserialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

/// The state of a session's cached title.
///
/// Distinguishes three states that `Option<String>` cannot: a title that was
/// never generated, one that loaded successfully, and one whose cache file
/// exists but could not be read. The title service uses this to avoid
/// attempting to save a title to a location that has already proven unwritable.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionTitle {
    /// No `.title` cache file exists — normal state for an untitled session.
    Absent,
    /// `.title` file was read successfully.
    Loaded(String),
    /// `.title` file exists but could not be read (permissions, corruption, etc.).
    Unreadable,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub uuid: String,
    pub jsonl_path: PathBuf,
    pub cwd: PathBuf,
    pub git_branch: Option<String>,
    pub first_message: Option<String>,
    pub title: SessionTitle,
    pub last_modified: SystemTime,
    pub size_bytes: u64,
    /// Set when the JSONL header could not be read (I/O error).
    /// `None` means the session loaded cleanly.
    pub parse_error: Option<String>,
}

impl Session {
    pub fn title_cache_path(&self) -> PathBuf {
        self.jsonl_path.with_extension("title")
    }

    pub fn needs_title(&self) -> bool {
        self.title == SessionTitle::Absent && self.first_message.is_some()
    }

    pub fn is_degraded(&self) -> bool {
        self.parse_error.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub label: String,
    pub sessions: Vec<Session>,
}

/// Load all projects from `base` (typically `~/.claude/projects`).
/// Called by `FsSessionStore::load`; the path is resolved there so this
/// function stays pure and testable without touching the home directory.
pub(crate) fn load_projects_from(base: &Path) -> anyhow::Result<Vec<Project>> {
    if !base.exists() {
        return Ok(vec![]);
    }

    let mut projects = Vec::new();

    for entry in std::fs::read_dir(base)? {
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

        let (cwd, git_branch, first_message, parse_error) = match parse_header(&path) {
            Ok((cwd, branch, msg)) => (cwd, branch, msg, None),
            Err(e) => (None, None, None, Some(e.to_string())),
        };

        let title = {
            let cp = path.with_extension("title");
            if cp.exists() {
                match std::fs::read_to_string(&cp) {
                    Ok(s) => {
                        let trimmed = s.trim().to_string();
                        if trimmed.is_empty() {
                            SessionTitle::Absent
                        } else {
                            SessionTitle::Loaded(trimmed)
                        }
                    }
                    Err(_) => SessionTitle::Unreadable,
                }
            } else {
                SessionTitle::Absent
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
            parse_error,
        });
    }

    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_modified));
    Ok(sessions)
}

fn parse_header(path: &Path) -> anyhow::Result<(Option<String>, Option<String>, Option<String>)> {
    let file = std::fs::File::open(path)?;
    parse_header_from_reader(std::io::BufReader::new(file))
}

/// Core parsing logic over any `BufRead` — testable without touching the filesystem.
fn parse_header_from_reader<R: BufRead>(
    reader: R,
) -> anyhow::Result<(Option<String>, Option<String>, Option<String>)> {
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

        let Ok(entry) = serde_json::from_str::<LogEntry>(&line) else {
            continue;
        };

        if cwd.is_none() {
            cwd = entry.cwd;
        }
        if branch.is_none() {
            branch = entry.git_branch;
        }

        if first_msg.is_none()
            && entry.kind.as_deref() == Some("user")
            && let Some(msg) = entry.message
            && msg.role.as_deref() == Some("user") {
                first_msg = extract_text(msg.content);
            }

        if cwd.is_some() && branch.is_some() && first_msg.is_some() {
            break;
        }
    }

    Ok((cwd, branch, first_msg))
}

fn extract_text(content: Option<MessageContent>) -> Option<String> {
    let raw = match content? {
        MessageContent::Text(s) => s,
        MessageContent::Blocks(blocks) => blocks
            .into_iter()
            .find(|b| b.kind == "text")
            .and_then(|b| b.text)?,
    };
    let cleaned = clean_message_text(&raw);
    if cleaned.is_empty() { None } else { Some(cleaned) }
}

/// Clean a raw JSONL message string for display and title generation:
/// 1. Strip XML/HTML-style tags (removes skill command markup like
///    `<command-message>` and `<command-name>`).
/// 2. Drop any line whose slash-prefixed form is the very next line —
///    skill commands produce a bare name followed by `/name`, so we keep
///    only the slash version to avoid "foo /foo" repetition.
fn clean_message_text(s: &str) -> String {
    // Step 1: strip tags, preserving inter-tag text and structure.
    let mut stripped = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => stripped.push(c),
            _ => {}
        }
    }

    // Step 2: remove a line when the immediately following line is "/" + that line.
    let lines: Vec<&str> = stripped.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let mut result = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let next_is_slash_version = lines
            .get(i + 1)
            .is_some_and(|next| *next == format!("/{line}"));
        if !next_is_slash_version {
            result.push(line);
        }
        i += 1;
    }

    result.join("\n").trim().to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn parse(jsonl: &str) -> (Option<String>, Option<String>, Option<String>) {
        parse_header_from_reader(Cursor::new(jsonl)).unwrap()
    }

    #[test]
    fn extracts_cwd_branch_and_string_content() {
        let line = r#"{"type":"user","cwd":"/home/user/proj","gitBranch":"main","message":{"role":"user","content":"hello world"}}"#;
        let (cwd, branch, msg) = parse(line);
        assert_eq!(cwd.as_deref(), Some("/home/user/proj"));
        assert_eq!(branch.as_deref(), Some("main"));
        assert_eq!(msg.as_deref(), Some("hello world"));
    }

    #[test]
    fn extracts_text_from_content_blocks() {
        let line = r#"{"type":"user","cwd":"/proj","gitBranch":"feat","message":{"role":"user","content":[{"type":"text","text":"  block message  "},{"type":"image"}]}}"#;
        let (_, _, msg) = parse(line);
        assert_eq!(msg.as_deref(), Some("block message"));
    }

    #[test]
    fn skips_non_user_entries_for_first_message() {
        let jsonl = "{ \"type\":\"assistant\",\"cwd\":\"/proj\",\"gitBranch\":\"main\",\"message\":{\"role\":\"assistant\",\"content\":\"response\"}}\n\
                     {\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"actual first\"}}";
        let (_, _, msg) = parse(jsonl);
        assert_eq!(msg.as_deref(), Some("actual first"));
    }

    #[test]
    fn cwd_and_branch_picked_up_from_any_line() {
        let jsonl = "{\"type\":\"system\",\"cwd\":\"/sys\",\"gitBranch\":\"dev\"}\n\
                     {\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}";
        let (cwd, branch, msg) = parse(jsonl);
        assert_eq!(cwd.as_deref(), Some("/sys"));
        assert_eq!(branch.as_deref(), Some("dev"));
        assert_eq!(msg.as_deref(), Some("hi"));
    }

    #[test]
    fn malformed_lines_are_skipped_gracefully() {
        let jsonl = "not json at all\n\
                     {\"type\":\"user\",\"cwd\":\"/ok\",\"gitBranch\":\"main\",\"message\":{\"role\":\"user\",\"content\":\"fine\"}}";
        let (cwd, _, msg) = parse(jsonl);
        assert_eq!(cwd.as_deref(), Some("/ok"));
        assert_eq!(msg.as_deref(), Some("fine"));
    }

    #[test]
    fn missing_fields_return_none() {
        let line = r#"{"type":"user","message":{"role":"user","content":"no cwd or branch"}}"#;
        let (cwd, branch, msg) = parse(line);
        assert!(cwd.is_none());
        assert!(branch.is_none());
        assert_eq!(msg.as_deref(), Some("no cwd or branch"));
    }

    #[test]
    fn content_block_without_text_type_is_ignored() {
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"image","url":"http://x"}]}}"#;
        let (_, _, msg) = parse(line);
        assert!(msg.is_none());
    }

    #[test]
    fn trims_whitespace_from_message() {
        let line = r#"{"type":"user","message":{"role":"user","content":"  \n  padded  \n  "}}"#;
        let (_, _, msg) = parse(line);
        assert_eq!(msg.as_deref(), Some("padded"));
    }

    #[test]
    fn strips_xml_tags_and_deduplicates_command_name() {
        // Skill messages: <command-message> gives bare name, <command-name> gives /name.
        // After cleaning, only the slash version should remain.
        let content = "<command-message>improve-codebase-architecture</command-message>\n\
                       <command-name>/improve-codebase-architecture</command-name>\n\
                       Base directory for this skill: /Users/sanjay/.claude/skills/";
        let line = format!(
            r#"{{"type":"user","message":{{"role":"user","content":"{content}"}}}}"#,
            content = content.replace('\n', "\\n").replace('"', "\\\"")
        );
        let (_, _, msg) = parse(&line);
        let msg = msg.unwrap();
        assert!(!msg.contains('<'), "should have no XML tags: {msg}");
        assert!(msg.starts_with("/improve-codebase-architecture"), "should start with slash command: {msg}");
        assert!(!msg.contains("improve-codebase-architecture /improve"), "should not have bare+slash repetition: {msg}");
        assert!(msg.contains("Base directory"), "should keep non-tag content: {msg}");
    }

    #[test]
    fn bare_command_name_without_slash_version_is_kept() {
        // If there's no following "/foo" line, "foo" is kept as-is.
        let content = "<command-message>foo</command-message>";
        let line = format!(
            r#"{{"type":"user","message":{{"role":"user","content":"{content}"}}}}"#,
        );
        let (_, _, msg) = parse(&line);
        assert_eq!(msg.as_deref(), Some("foo"));
    }

    // ── SessionTitle / needs_title / is_degraded ──────────────────────────────

    fn make_session(title: SessionTitle, first_message: Option<&str>, parse_error: Option<&str>) -> Session {
        Session {
            uuid: "test-uuid".into(),
            jsonl_path: std::path::PathBuf::from("/tmp/test.jsonl"),
            cwd: std::path::PathBuf::from("/tmp"),
            git_branch: None,
            first_message: first_message.map(String::from),
            title,
            last_modified: std::time::SystemTime::UNIX_EPOCH,
            size_bytes: 0,
            parse_error: parse_error.map(String::from),
        }
    }

    #[test]
    fn needs_title_true_when_absent_and_has_message() {
        let s = make_session(SessionTitle::Absent, Some("hello"), None);
        assert!(s.needs_title());
    }

    #[test]
    fn needs_title_false_when_title_loaded() {
        let s = make_session(SessionTitle::Loaded("My Title".into()), Some("hello"), None);
        assert!(!s.needs_title());
    }

    #[test]
    fn needs_title_false_when_no_first_message() {
        let s = make_session(SessionTitle::Absent, None, None);
        assert!(!s.needs_title());
    }

    #[test]
    fn needs_title_false_when_title_unreadable() {
        // Unreadable means the cache file exists but is corrupt — don't retry.
        let s = make_session(SessionTitle::Unreadable, Some("hello"), None);
        assert!(!s.needs_title());
    }

    #[test]
    fn is_degraded_false_for_clean_session() {
        let s = make_session(SessionTitle::Absent, Some("hello"), None);
        assert!(!s.is_degraded());
    }

    #[test]
    fn is_degraded_true_when_parse_error_set() {
        let s = make_session(SessionTitle::Absent, None, Some("failed to open file: permission denied"));
        assert!(s.is_degraded());
    }
}
