//! Export Claude desktop Code-tab chats to Markdown.
//!
//! The desktop app keeps one `local_*.json` per chat (title, folder, CLI
//! session id) under `%APPDATA%\Claude\claude-code-sessions\<account>\<org>\`.
//! The conversation itself is the Claude Code transcript
//! `~\.claude\projects\<project>\<cliSessionId>.jsonl`.


use chrono::{DateTime, Local, TimeZone};
use serde_json::Value;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Session {
    pub id: String,
    pub cli_id: String,
    pub title: String,
    pub cwd: String,
    pub last: i64,
    pub archived: bool,
    /// Model last picked for this chat in the app, e.g. `claude-opus-5-5`.
    pub model: String,
}

pub fn load_sessions() -> Result<Vec<Session>, String> {
    let appdata = std::env::var("APPDATA").map_err(|_| "APPDATA nicht gesetzt")?;
    let root = Path::new(&appdata).join("Claude").join("claude-code-sessions");
    let mut out = vec![];
    // <account>/<org>/local_*.json
    for account in read_dirs(&root) {
        for org in read_dirs(&account) {
            for f in fs::read_dir(&org).into_iter().flatten().flatten() {
                let p = f.path();
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !(name.starts_with("local_") && name.ends_with(".json")) {
                    continue;
                }
                let Some(v) = fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
                    continue;
                };
                let Some(cli_id) = v["cliSessionId"].as_str() else { continue };
                out.push(Session {
                    id: v["sessionId"].as_str().unwrap_or(name).to_string(),
                    cli_id: cli_id.to_string(),
                    title: v["title"].as_str().unwrap_or("(ohne Titel)").to_string(),
                    cwd: v["cwd"].as_str().unwrap_or("").to_string(),
                    last: v["lastActivityAt"].as_i64().or(v["createdAt"].as_i64()).unwrap_or(0),
                    archived: v["isArchived"].as_bool().unwrap_or(false),
                    model: v["model"].as_str().unwrap_or("").to_string(),
                });
            }
        }
    }
    if out.is_empty() {
        return Err(format!("keine Chats unter {} gefunden", root.display()));
    }
    Ok(out)
}

fn read_dirs(p: &Path) -> Vec<PathBuf> {
    fs::read_dir(p)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

pub fn find_transcript(cli_id: &str) -> Option<PathBuf> {
    let base = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".claude")))?;
    let file = format!("{cli_id}.jsonl");
    read_dirs(&base.join("projects")).into_iter().map(|d| d.join(&file)).find(|p| p.is_file())
}

/// Renders one chat as Markdown; `None` when its transcript is gone.
pub fn export(s: &Session, tools: bool) -> Result<Option<String>, String> {
    let Some(path) = find_transcript(&s.cli_id) else { return Ok(None) };
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;

    let mut md = String::new();
    let mut role = "";
    // Model of the current reply, the one named in the last heading, and every model used.
    let mut model = String::new();
    let mut heading_model = String::new();
    let mut used: Vec<(String, usize)> = vec![];
    for line in text.lines() {
        let Ok(rec) = serde_json::from_str::<Value>(line) else { continue };
        let kind = rec["type"].as_str().unwrap_or("");
        if !(kind == "user" || kind == "assistant") || rec["isSidechain"] == true || rec["isMeta"] == true {
            continue;
        }
        if rec["isCompactSummary"] == true {
            md.push_str("\n---\n*Hier wurde der Verlauf zusammengefasst (Kontext voll).*\n---\n");
            continue;
        }
        let blocks = match &rec["message"]["content"] {
            Value::String(t) => vec![serde_json::json!({"type": "text", "text": t})],
            Value::Array(a) => a.clone(),
            _ => continue,
        };
        let time = rec["timestamp"].as_str().map(fmt_iso).unwrap_or_default();
        // `<synthetic>` marks messages the app wrote itself, not a model.
        if let Some(m) = rec["message"]["model"].as_str().filter(|m| kind == "assistant" && !m.starts_with('<')) {
            model = m.to_string();
        }

        for b in &blocks {
            let piece = match b["type"].as_str().unwrap_or("") {
                "text" => {
                    let t = clean_text(b["text"].as_str().unwrap_or(""));
                    if t.is_empty() {
                        continue;
                    }
                    // Only real text decides whose turn it is; tool results ride along.
                    let who = if kind == "user" { "Du" } else { "Claude" };
                    if who == "Claude" && (role != who || heading_model != model) {
                        match used.iter_mut().find(|(m, _)| *m == model) {
                            Some((_, n)) => *n += 1,
                            None => used.push((model.clone(), 1)),
                        }
                        let _ = write!(md, "\n## Claude · {} · {time}\n\n", model_name(&model));
                        heading_model = model.clone();
                    } else if role != who {
                        let _ = write!(md, "\n## {who} · {time}\n\n");
                    }
                    role = who;
                    t
                }
                "image" if kind == "user" => "*[Bild]*".to_string(),
                "tool_use" if tools => {
                    format!("> Werkzeug **{}** {}", b["name"].as_str().unwrap_or("?"), tool_summary(&b["input"]))
                }
                "tool_result" if tools => {
                    let body = truncate(&result_text(&b["content"]), 1500);
                    format!("<details><summary>Ergebnis</summary>\n\n```\n{}\n```\n</details>", body.replace("```", "ˋˋˋ"))
                }
                _ => continue,
            };
            md.push_str(&piece);
            md.push_str("\n\n");
        }
    }

    let mut head = String::new();
    let _ = writeln!(head, "# {}\n", s.title);
    let _ = writeln!(head, "- Ordner: `{}`", s.cwd);
    let _ = writeln!(head, "- Zuletzt aktiv: {}", fmt_ms(s.last, "%d.%m.%Y %H:%M"));
    if !used.is_empty() {
        let list: Vec<String> = used
            .iter()
            .map(|(m, n)| format!("{} (`{m}`, {n} {})", model_name(m), if *n == 1 { "Antwort" } else { "Antworten" }))
            .collect();
        let _ = writeln!(head, "- Modell: {}", list.join(", "));
    }
    let _ = writeln!(head, "- Session: `{}`\n", s.cli_id);
    Ok(Some(head + &md))
}

/// `claude-opus-5-5` -> `Opus 5.5`, `claude-3-5-sonnet-20241022` -> `Sonnet 3.5`.
/// Ids that do not look like Claude models come back unchanged.
pub fn model_name(id: &str) -> String {
    if id.is_empty() {
        return "unbekanntes Modell".into();
    }
    let core = id.split('[').next().unwrap_or(id);
    let Some(rest) = core.strip_prefix("claude-") else { return id.to_string() };
    let mut family = String::new();
    let mut version = vec![];
    for part in rest.split('-') {
        if part.chars().all(|c| c.is_ascii_digit()) {
            // Snapshot dates such as 20241022 are not part of the version.
            if part.len() < 8 {
                version.push(part);
            }
        } else if family.is_empty() {
            let mut c = part.chars();
            family = c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default();
        }
    }
    match (family.is_empty(), version.is_empty()) {
        (true, _) => id.to_string(),
        (false, true) => family,
        (false, false) => format!("{family} {}", version.join(".")),
    }
}

#[cfg(test)]
mod tests {
    use super::model_name;

    #[test]
    fn model_names() {
        assert_eq!(model_name("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(model_name("claude-opus-4-8"), "Opus 4.8");
        assert_eq!(model_name("claude-sonnet-4-5-20250929"), "Sonnet 4.5");
        assert_eq!(model_name("claude-3-5-sonnet-20241022"), "Sonnet 3.5");
        assert_eq!(model_name("claude-fable-5-1[1m]"), "Fable 5.1");
        assert_eq!(model_name("gpt-x"), "gpt-x");
        assert_eq!(model_name(""), "unbekanntes Modell");
    }
}

/// Drops the harness wrappers the app adds around real messages.
fn clean_text(t: &str) -> String {
    let mut s = strip_tag(t, "system-reminder");
    for tag in ["local-command-stdout", "local-command-caveat", "task-notification"] {
        s = strip_tag(&s, tag);
    }
    // Slash commands are stored as <command-name>/x</command-name><command-args>…
    if let Some(name) = between(&s, "<command-name>", "</command-name>") {
        let args = between(&s, "<command-args>", "</command-args>").unwrap_or("");
        return format!("`{} {}`", name.trim(), args.trim()).replace(" `", "`");
    }
    s.trim().to_string()
}

fn strip_tag(t: &str, tag: &str) -> String {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut out = String::new();
    let mut rest = t;
    while let Some(i) = rest.find(&open) {
        out.push_str(&rest[..i]);
        match rest[i..].find(&close) {
            Some(j) => rest = &rest[i + j + close.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn between<'a>(t: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let i = t.find(a)? + a.len();
    let j = t[i..].find(b)?;
    Some(&t[i..i + j])
}

fn tool_summary(input: &Value) -> String {
    for key in ["description", "command", "file_path", "pattern", "url", "query", "prompt"] {
        if let Some(v) = input[key].as_str() {
            return format!("— {}", truncate(&v.replace('\n', " "), 160));
        }
    }
    String::new()
}

fn result_text(c: &Value) -> String {
    match c {
        Value::String(s) => strip_tag(s, "system-reminder").trim().to_string(),
        Value::Array(a) => a
            .iter()
            .map(|b| match b["type"].as_str() {
                Some("text") => b["text"].as_str().unwrap_or("").to_string(),
                Some("image") => "[Bild]".to_string(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{} …", &s[..i]),
        None => s.to_string(),
    }
}

pub fn safe_name(t: &str) -> String {
    let s: String = t
        .chars()
        .map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' })
        .collect();
    truncate(s.trim(), 80).trim_end_matches(" …").to_string()
}

pub fn fmt_ms(ms: i64, f: &str) -> String {
    Local.timestamp_millis_opt(ms).single().map(|d| d.format(f).to_string()).unwrap_or_default()
}

fn fmt_iso(s: &str) -> String {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Local).format("%d.%m.%Y %H:%M").to_string())
        .unwrap_or_default()
}
