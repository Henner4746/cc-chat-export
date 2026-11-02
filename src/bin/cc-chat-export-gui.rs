//! Window front end: pick a chat, preview it, save it as Markdown.

#![windows_subsystem = "windows"]

use cc_chat_export::{export, fmt_ms, load_sessions, safe_name, Session};
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Claude Chat Export")
            .with_inner_size([1100.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native("Claude Chat Export", options, Box::new(|_| Ok(Box::new(App::new()))))
}

struct App {
    sessions: Vec<Session>,
    filter: String,
    archived: bool,
    tools: bool,
    selected: Option<usize>,
    /// Markdown of the selected chat, rebuilt when selection or `tools` changes.
    preview: String,
    preview_key: Option<(usize, bool)>,
    status: String,
}

impl App {
    fn new() -> Self {
        let mut app = App {
            sessions: vec![],
            filter: String::new(),
            archived: false,
            tools: false,
            selected: None,
            preview: String::new(),
            preview_key: None,
            status: String::new(),
        };
        app.reload();
        // cc-chat-export-gui [--tools] [--open TITLE] [SEARCH]
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--tools" => app.tools = true,
                "--open" => {
                    let q = args.next().unwrap_or_default().to_lowercase();
                    app.selected = app.sessions.iter().position(|s| s.title.to_lowercase().contains(&q));
                }
                _ => app.filter = a,
            }
        }
        app
    }

    fn reload(&mut self) {
        match load_sessions() {
            Ok(mut s) => {
                s.sort_by(|a, b| b.last.cmp(&a.last));
                self.status = format!("{} Chats gefunden", s.len());
                self.sessions = s;
            }
            Err(e) => self.status = e,
        }
        self.selected = None;
        self.preview_key = None;
        self.preview.clear();
    }

    fn visible(&self) -> Vec<usize> {
        let q = self.filter.to_lowercase();
        (0..self.sessions.len())
            .filter(|&i| {
                let s = &self.sessions[i];
                (self.archived || !s.archived) && (q.is_empty() || s.title.to_lowercase().contains(&q))
            })
            .collect()
    }

    fn refresh_preview(&mut self) {
        let Some(i) = self.selected else { return };
        if self.preview_key == Some((i, self.tools)) {
            return;
        }
        self.preview = match export(&self.sessions[i], self.tools) {
            Ok(Some(md)) => md,
            Ok(None) => "Der Verlauf dieses Chats ist nicht mehr vorhanden.".into(),
            Err(e) => format!("Fehler: {e}"),
        };
        self.preview_key = Some((i, self.tools));
    }

    fn save_selected(&mut self) {
        let Some(i) = self.selected else { return };
        let s = &self.sessions[i];
        let name = format!("{} {}.md", fmt_ms(s.last, "%Y-%m-%d"), safe_name(&s.title));
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).add_filter("Markdown", &["md"]).save_file() else {
            return;
        };
        self.status = match std::fs::write(&path, &self.preview) {
            Ok(()) => format!("Gespeichert: {}", path.display()),
            Err(e) => format!("Fehler: {e}"),
        };
    }

    fn save_all(&mut self) {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
        let (mut ok, mut missing) = (0, 0);
        for i in self.visible() {
            let s = &self.sessions[i];
            match export(s, self.tools) {
                Ok(Some(md)) => {
                    let name = format!("{} {}.md", fmt_ms(s.last, "%Y-%m-%d"), safe_name(&s.title));
                    if let Err(e) = std::fs::write(dir.join(name), md) {
                        self.status = format!("Fehler: {e}");
                        return;
                    }
                    ok += 1;
                }
                _ => missing += 1,
            }
        }
        self.status = format!("{ok} Chats nach {} exportiert ({missing} ohne Verlauf übersprungen)", dir.display());
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::Panel::top("bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Suche:");
                ui.text_edit_singleline(&mut self.filter);
                ui.checkbox(&mut self.archived, "Archivierte");
                ui.checkbox(&mut self.tools, "Werkzeugaufrufe");
                if ui.button("Neu laden").clicked() {
                    self.reload();
                }
                if ui.button("Alle sichtbaren exportieren…").clicked() {
                    self.save_all();
                }
            });
        });

        egui::Panel::bottom("status").show(ui, |ui| ui.label(&self.status));

        egui::Panel::left("chats").resizable(true).default_size(360.0).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                for i in self.visible() {
                    let s = &self.sessions[i];
                    let text = format!("{}\n{}", s.title, fmt_ms(s.last, "%d.%m.%Y %H:%M"));
                    if ui.selectable_label(self.selected == Some(i), text).clicked() {
                        self.selected = Some(i);
                    }
                }
            });
        });

        self.refresh_preview();

        egui::CentralPanel::default().show(ui, |ui| {
            if self.selected.is_none() {
                ui.centered_and_justified(|ui| ui.label("Links einen Chat auswählen"));
                return;
            }
            ui.horizontal(|ui| {
                if ui.button("Als Markdown speichern…").clicked() {
                    self.save_selected();
                }
                if ui.button("In Zwischenablage kopieren").clicked() {
                    ctx.copy_text(self.preview.clone());
                    self.status = "Kopiert".into();
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.preview.as_str())
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
            });
        });
    }
}
