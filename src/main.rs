//! Command-line front end.

use cc_chat_export::{export, fmt_ms, load_sessions, safe_name, Session};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
cc-chat-export: Claude-Desktop-Code-Chats als Markdown exportieren

  cc-chat-export [list] [--archived]        Chats auflisten (neueste zuerst)
  cc-chat-export <auswahl> [-o DATEI] [--tools]
        <auswahl> = Nummer aus der Liste, Session-ID (Anfang reicht) oder Teil des Titels
  cc-chat-export --all -o ORDNER [--archived] [--tools]

  --tools     Werkzeugaufrufe und gekürzte Ergebnisse mit ausgeben
  --archived  archivierte Chats einbeziehen";
struct Opts {
    tools: bool,
    archived: bool,
    all: bool,
    out: Option<PathBuf>,
    rest: Vec<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Fehler: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let opts = parse_args()?;
    let mut sessions = load_sessions()?;
    sessions.sort_by(|a, b| b.last.cmp(&a.last));
    let visible: Vec<&Session> = sessions
        .iter()
        .filter(|s| opts.archived || !s.archived)
        .collect();

    if opts.all {
        let dir = opts.out.as_ref().ok_or("--all braucht -o ORDNER")?;
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut n = 0;
        for s in &visible {
            let Some(md) = export(s, opts.tools)? else { continue };
            let name = format!("{} {}.md", fmt_ms(s.last, "%Y-%m-%d"), safe_name(&s.title));
            fs::write(dir.join(&name), md).map_err(|e| format!("{name}: {e}"))?;
            n += 1;
        }
        println!("{n} Chats nach {} exportiert", dir.display());
        return Ok(());
    }

    match opts.rest.first().map(String::as_str) {
        None | Some("list") => {
            for (i, s) in visible.iter().enumerate() {
                let mark = if s.archived { " [archiviert]" } else { "" };
                println!("{:>3}  {}  {}{}", i + 1, fmt_ms(s.last, "%d.%m.%Y %H:%M"), s.title, mark);
            }
            Ok(())
        }
        Some(sel) => {
            let s = select(&sessions, &visible, sel)?;
            let md = export(s, opts.tools)?
                .ok_or_else(|| format!("kein Verlauf für {} gefunden", s.cli_id))?;
            match &opts.out {
                Some(p) => {
                    fs::write(p, md).map_err(|e| format!("{}: {e}", p.display()))?;
                    eprintln!("„{}“ → {}", s.title, p.display());
                }
                None => print!("{md}"),
            }
            Ok(())
        }
    }
}

fn parse_args() -> Result<Opts, String> {
    let mut o = Opts { tools: false, archived: false, all: false, out: None, rest: vec![] };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--tools" => o.tools = true,
            "--archived" => o.archived = true,
            "--all" => o.all = true,
            "-o" | "--out" => o.out = Some(args.next().ok_or("-o braucht einen Pfad")?.into()),
            _ if a.starts_with('-') => return Err(format!("unbekannte Option {a}\n\n{USAGE}")),
            _ => o.rest.push(a),
        }
    }
    Ok(o)
}

fn select<'a>(all: &'a [Session], visible: &[&'a Session], sel: &str) -> Result<&'a Session, String> {
    if let Ok(n) = sel.parse::<usize>() {
        return visible.get(n.wrapping_sub(1)).copied().ok_or_else(|| format!("keine Nummer {n}"));
    }
    let id = sel.trim_start_matches("local_");
    if let Some(s) = all.iter().find(|s| s.id.trim_start_matches("local_").starts_with(id) || s.cli_id.starts_with(id)) {
        return Ok(s);
    }
    let q = sel.to_lowercase();
    let hits: Vec<&Session> = all.iter().filter(|s| s.title.to_lowercase().contains(&q)).collect();
    match hits.as_slice() {
        [s] => Ok(s),
        [] => Err(format!("kein Chat passt zu „{sel}“")),
        _ => {
            let names: Vec<_> = hits.iter().map(|s| format!("  {}", s.title)).collect();
            Err(format!("„{sel}“ ist mehrdeutig:\n{}", names.join("\n")))
        }
    }
}

