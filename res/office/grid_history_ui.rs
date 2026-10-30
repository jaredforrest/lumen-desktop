//! Captured workbook-owned local version decisions.
use crate::SheetApp;
use serde_json::{Value, json};
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct HistoryDialog {
    rows: Vec<Value>,
    selected: String,
    preview: String,
    error: String,
    label: String,
    confirm: bool,
    #[serde(skip)]
    owner: Option<Owner>,
    #[serde(skip)]
    password: gridcraft_engine::HistoryPassword,
    protected: Option<bool>,
}
#[derive(Clone, Debug)]
struct Owner {
    uid: u64,
    revision: u64,
    path: Option<String>,
}
impl Owner {
    fn capture(app: &SheetApp) -> Option<Self> {
        app.session.active().map(|d| Self { uid: d.uid, revision: d.revision, path: d.path.clone() })
    }
    fn matches(&self, app: &SheetApp) -> bool {
        app.session.active().is_some_and(|d| self.uid == d.uid && self.revision == d.revision && self.path == d.path)
    }
}
impl HistoryDialog {
    pub(crate) fn new(app: &mut SheetApp) -> Self {
        let mut state = Self {
            rows: Vec::new(),
            selected: String::new(),
            preview: String::new(),
            error: String::new(),
            label: "Saved version".into(),
            confirm: false,
            owner: Owner::capture(app),
            password: gridcraft_engine::HistoryPassword::default(),
            protected: None,
        };
        state.reload(app);
        state
    }
    fn command(&mut self, app: &mut SheetApp, action: &str) -> Result<Value, String> {
        let owner =
            self.owner.as_ref().filter(|o| o.matches(app)).ok_or("This history request is no longer current. Close and reopen Version History.")?;
        let p = json!({"action":action,"id":self.selected,"label":self.label,"document":owner.uid,"revision":owner.revision,"allowDiscard":action=="restore"&&self.confirm,"historyPassword":self.password.as_str()});
        if action == "restore" {
            self.owner = None;
        }
        app.session.execute("file.history", p).map_err(|e| e.to_string())
    }
    fn reload(&mut self, app: &mut SheetApp) {
        self.rows.clear();
        self.selected.clear();
        self.preview.clear();
        self.confirm = false;
        self.protected = None;
        match self.command(app, "list") {
            Ok(v) => {
                self.rows = v["versions"].as_array().cloned().unwrap_or_default();
                self.protected = v["protected"].as_bool();
                self.error.clear();
            }
            Err(e) => self.error = e,
        }
    }
    pub(crate) fn body(&mut self, app: &mut SheetApp, ui: &mut egui::Ui) -> bool {
        let current = self.owner.as_ref().is_some_and(|o| o.matches(app));
        let mut close = false;
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                if current && self.confirm {
                    if ui.button("Cancel restoration").clicked() {
                        self.confirm = false;
                    }
                    if ui.button("Restore Version").clicked() {
                        match self.command(app, "restore") {
                            Ok(_) => close = true,
                            Err(e) => {
                                self.error = e;
                                self.confirm = false;
                            }
                        }
                    }
                } else {
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                    if current && ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Restore selected version…")).clicked() {
                        self.confirm = true;
                    }
                }
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical().id_salt("workbook-history-content").auto_shrink([false, false]).show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| self.contents(app, ui, current));
            });
        });
        close
    }
    fn contents(&mut self, app: &mut SheetApp, ui: &mut egui::Ui, current: bool) {
        ui.label("Versions are stored locally beside this saved workbook. Include its history folder in backups. Moving or renaming the source does not move history.");
        ui.label("Explicit captures only: up to 20 versions and 32 MiB per workbook. Oldest versions expire at these limits. This is not an automatic backup.");
        ui.label(match self.protected {
            Some(true) => "History archive is password protected.",
            Some(false) => "History archive is not password protected.",
            None => "History protection could not be checked. Enter its archive password and refresh when needed.",
        });
        ui.label("The optional history password protects this archive only. It does not encrypt the source workbook. Capturing with a password encrypts the whole archive; existing protection is retained.");
        if !self.error.is_empty() {
            ui.label(&self.error);
        }
        if !current {
            ui.label("This history request is no longer current. Close and reopen Version History.");
            return;
        }
        if self.confirm {
            ui.label("Restore this version into the captured working workbook? Unsaved changes will be replaced. A Before restore version is published first. The saved source remains unchanged until you explicitly save.");
            return;
        }
        ui.horizontal(|ui| {
            let label = ui.label("History password:");
            ui.add(egui::TextEdit::singleline(self.password.as_mut_string()).password(true).desired_width(180.0)).labelled_by(label.id);
        });
        ui.horizontal(|ui| {
            let label = ui.label("Version label:");
            ui.text_edit_singleline(&mut self.label).labelled_by(label.id);
            if ui.button("Capture version").clicked() {
                match self.command(app, "capture") {
                    Ok(_) => self.reload(app),
                    Err(e) => self.error = e,
                }
            }
        });
        if ui.button("Refresh history").clicked() {
            self.reload(app);
        }
        let mut selected = None;
        for row in &self.rows {
            let id = row["id"].as_str().unwrap_or("");
            let text = format!(
                "{} · {} · {}",
                row["label"].as_str().unwrap_or("Version"),
                row["date"].as_str().unwrap_or(""),
                row["summary"].as_str().unwrap_or("")
            );
            if ui.selectable_label(self.selected == id, text).clicked() {
                selected = Some(id.to_owned());
            }
        }
        if self.rows.is_empty() {
            ui.label("No persistent versions have been captured for this saved workbook.");
        }
        if let Some(id) = selected {
            self.selected = id;
            self.preview.clear();
            self.confirm = false;
            match self.command(app, "preview") {
                Ok(v) => {
                    self.preview = v["text"].as_str().unwrap_or("").into();
                    self.error.clear();
                }
                Err(e) => {
                    self.error = e;
                    self.protected = None;
                }
            }
        }
        if !self.preview.is_empty() {
            ui.label(&self.preview);
        }
    }
}
#[cfg(test)]
mod persistent_history_ui_tests {
    use super::*;
    use crate::Services;
    use gridcraft_engine::Session;
    fn app() -> SheetApp {
        let mut session = Session::new();
        session.new_workbook();
        SheetApp::new(session, Services::default())
    }
    #[test]
    fn persistent_history_ui_rejects_stale_workbook_and_path_and_redacts_password() {
        let mut app = app();
        let mut state = HistoryDialog::new(&mut app);
        state.password = gridcraft_engine::HistoryPassword::new("Synthetic archive password");
        assert!(!format!("{state:?}").contains("Synthetic archive password"));
        assert!(!serde_json::to_string(&state).unwrap().contains("Synthetic archive password"));
        app.session.active_mut().unwrap().revision += 1;
        assert!(state.command(&mut app, "capture").unwrap_err().contains("no longer current"));
        let mut state = HistoryDialog::new(&mut app);
        app.session.active_mut().unwrap().path = Some("renamed.xlsx".into());
        assert!(state.command(&mut app, "restore").is_err());
    }
    #[test]
    fn persistent_history_ui_failed_restore_is_one_shot() {
        let mut app = app();
        let mut state = HistoryDialog::new(&mut app);
        state.selected = "invalid".into();
        state.confirm = true;
        assert!(state.command(&mut app, "restore").is_err());
        assert!(state.owner.is_none());
        assert!(state.command(&mut app, "restore").unwrap_err().contains("no longer current"));
    }
}

#[cfg(test)]
mod persistent_history_widget_tests {
    use super::*;
    use crate::Services;
    use egui_kittest::kittest::Queryable;
    use gridcraft_engine::Session;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("grid-history-widget-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn app(&self) -> SheetApp {
            let mut session = Session::new();
            session.new_workbook();
            session.execute("cell.set", json!({"cell":"A1","input":"Original history cell"})).unwrap();
            session.execute("file.saveAs", json!({"path":self.0.join("source.xlsx")})).unwrap();
            let mut app = SheetApp::new(session, Services::default());
            app.run("file.history", json!({})).unwrap();
            app
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn state(app: &SheetApp) -> &HistoryDialog {
        app.dialog.as_ref().unwrap().history.as_ref().unwrap()
    }
    #[test]
    fn persistent_history_widgets_capture_select_preview_cancel_restore_and_fixed_footer() {
        for size in [egui::vec2(1100.0, 900.0), egui::vec2(700.0, 480.0)] {
            let root = Fixture::new();
            let app = root.app();
            let original = (*app.session.active().unwrap().wb).clone();
            let source = std::fs::read(root.0.join("source.xlsx")).unwrap();
            let mut h = egui_kittest::Harness::builder().with_size(size).build_ui_state(
                |ui, app: &mut SheetApp| {
                    let ctx = ui.ctx().clone();
                    app.logic(&ctx);
                    app.ui(ui);
                },
                app,
            );
            h.run_steps(4);
            h.get_by_label("Capture version").click();
            h.run_steps(4);
            assert_eq!(state(h.state()).rows.len(), 1);
            let row = &state(h.state()).rows[0];
            let label = format!("{} · {} · {}", row["label"].as_str().unwrap(), row["date"].as_str().unwrap(), row["summary"].as_str().unwrap());
            h.get_by_label(&label).click();
            h.run_steps(4);
            assert!(state(h.state()).preview.contains("Original history cell"));
            h.get_by_label("Close").click();
            h.run_steps(4);
            assert!(h.state().dialog.is_none());
            h.state_mut().session.execute("cell.set", json!({"cell":"A1","input":"Later unsaved cell"})).unwrap();
            let edited = (*h.state().session.active().unwrap().wb).clone();
            h.state_mut().run("file.history", json!({})).unwrap();
            h.run_steps(4);
            h.get_by_label(&label).click();
            h.run_steps(4);
            h.get_by_label("Restore selected version…").click();
            h.run_steps(4);
            assert!(state(h.state()).confirm);
            h.get_by_label("Cancel restoration").click();
            h.run_steps(4);
            assert_eq!(*h.state().session.active().unwrap().wb, edited);
            h.get_by_label("Restore selected version…").click();
            h.run_steps(4);
            h.get_by_label("Restore Version").click();
            h.run_steps(4);
            assert!(h.state().dialog.is_none());
            assert_eq!(*h.state().session.active().unwrap().wb, original);
            assert!(h.state().session.active().unwrap().is_dirty());
            assert_eq!(std::fs::read(root.0.join("source.xlsx")).unwrap(), source);
            h.state_mut().run("file.history", json!({})).unwrap();
            h.run_steps(4);
            let initial = h.get_by_label("Close").rect();
            let state = h.state_mut().dialog.as_mut().unwrap().history.as_mut().unwrap();
            state.rows = (0..20)
                .map(|i| json!({"id":format!("{i:x}"),"label":"Recovery snapshot","date":"2026-01-01T00:00:00Z","summary":"500 slides"}))
                .collect();
            state.selected = "0".into();
            state.preview = "A retained recovery preview.\n".repeat(80);
            h.run_steps(4);
            assert_eq!(initial, h.get_by_label("Close").rect());
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            assert!(viewport.contains_rect(h.get_by_label("Close").rect()));
            assert!(viewport.contains_rect(h.get_by_label("Restore selected version…").rect()));
            h.get_by_label("Restore selected version…").click();
            h.run_steps(4);
            assert!(viewport.contains_rect(h.get_by_label("Cancel restoration").rect()));
            assert!(viewport.contains_rect(h.get_by_label("Restore Version").rect()));
            h.get_by_label("Cancel restoration").click();
            h.run_steps(4);
            h.get_by_label("Close").click();
            h.run_steps(4);
            assert!(h.state().dialog.is_none());
        }
    }
}

#[cfg(test)]
mod persistent_history_preview_failure_tests {
    use super::*;
    use crate::Services;
    use egui_kittest::kittest::Queryable;
    use gridcraft_engine::Session;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn state(app: &SheetApp) -> &HistoryDialog {
        app.dialog.as_ref().unwrap().history.as_ref().unwrap()
    }
    #[test]
    fn persistent_history_widgets_failed_new_selection_clears_old_preview_and_protection() {
        let password = "Synthetic preview failure archive password";
        for size in [egui::vec2(1100.0, 900.0), egui::vec2(700.0, 480.0)] {
            let root = Fixture(std::env::temp_dir().join(format!(
                "grid-history-preview-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )));
            std::fs::create_dir(&root.0).unwrap();
            let mut session = Session::new();
            session.new_workbook();
            session
                .execute(
                    "cell.set",
                    json!({"cell":"A1","input":"Original version A preview"}),
                )
                .unwrap();
            session
                .execute("file.saveAs", json!({"path":root.0.join("source.xlsx")}))
                .unwrap();
            let first = session
                .execute(
                    "file.history",
                    json!({"action":"capture","label":"Version A","historyPassword":password}),
                )
                .unwrap()["versions"][0]
                .clone();
            session
                .execute(
                    "cell.set",
                    json!({"cell":"A1","input":"Different version B preview"}),
                )
                .unwrap();
            let second = session
                .execute(
                    "file.history",
                    json!({"action":"capture","label":"Version B","historyPassword":password}),
                )
                .unwrap()["versions"][0]
                .clone();
            let mut app = SheetApp::new(session, Services::default());
            app.run("file.history", json!({})).unwrap();
            let mut h = egui_kittest::Harness::builder()
                .with_size(size)
                .build_ui_state(
                    |ui, app: &mut SheetApp| {
                        let ctx = ui.ctx().clone();
                        app.logic(&ctx);
                        app.ui(ui);
                    },
                    app,
                );
            h.run_steps(4);
            h.get_by_label("History password:").click();
            h.get_by_label("History password:").type_text(password);
            h.run_steps(4);
            h.get_by_label("Refresh history").click();
            h.run_steps(4);
            assert_eq!(state(h.state()).protected, Some(true));
            assert_eq!(state(h.state()).rows.len(), 2);
            let label = |row: &Value| {
                format!(
                    "{} · {} · {}",
                    row["label"].as_str().unwrap(),
                    row["date"].as_str().unwrap(),
                    row["summary"].as_str().unwrap()
                )
            };
            let first_label = label(&first);
            h.get_by_label(&first_label).scroll_to_me();
            h.run_steps(4);
            h.get_by_label(&first_label).click();
            h.run_steps(4);
            assert!(
                state(h.state())
                    .preview
                    .contains("Original version A preview")
            );
            h.get_by_label("History password:").scroll_to_me();
            h.run_steps(4);
            h.get_by_label("History password:").click();
            h.get_by_label("History password:").type_text("wrong");
            h.run_steps(4);
            let second_label = label(&second);
            h.get_by_label(&second_label).scroll_to_me();
            h.run_steps(4);
            h.get_by_label(&second_label).click();
            h.run_steps(4);
            let state = state(h.state());
            assert_eq!(state.selected, second["id"].as_str().unwrap());
            assert!(state.preview.is_empty());
            assert!(!state.confirm);
            assert_eq!(state.protected, None);
            assert!(state.error.contains("incorrect") || state.error.contains("damaged"));
        }
    }
}
