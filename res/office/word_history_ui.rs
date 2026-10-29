//! Captured document-owned browsing; restoration never saves over the source.
use std::path::PathBuf;
use serde::Serialize;
use serde_json::{Value,json};
use crate::{WordApp,dialogs::Dialog};
use wordcraft_engine::Password;
#[derive(Clone,Debug,Serialize)]
pub struct HistoryDialog {
    rows: Vec<Value>, selected: String, preview: String, error: String,
    label: String, protected: Option<bool>, confirm: bool,
    #[serde(skip)] password: Password,
}
pub(crate) struct HistoryOwner { document: u64, revision: u64, path: Option<PathBuf>, password: Option<Password> }
impl HistoryOwner {
    fn capture(app:&WordApp)->Self { Self {document:app.session.document_id(),revision:app.session.rev(),path:app.session.path.clone(),password:app.session.password.clone()} }
    fn matches(&self,app:&WordApp)->bool {self.document==app.session.document_id() && self.revision==app.session.rev() && self.path==app.session.path && self.password==app.session.password}
}
impl WordApp {
    pub(crate) fn open_history(&mut self) {
        if self.dialog.is_some() || self.file_dialog_open() || self.conversion.is_some() { self.status("Another document decision is already open.");return; }
        self.history_owner=Some(HistoryOwner::capture(self));
        let mut state=HistoryDialog {rows:Vec::new(),selected:String::new(),preview:String::new(),error:String::new(),label:"Saved version".into(),protected:None,confirm:false,password:Password::default()};
        reload(self,&mut state);self.dialog=Some(Dialog::VersionHistory {state});self.ui.backstage=false;self.canvas.focused=false;
    }
    fn history_command(&mut self, state:&HistoryDialog, action:&str)->Result<Value,String> {
        let owner=self.history_owner.as_ref().filter(|owner|owner.matches(self)).ok_or("This history request is no longer current. Close and reopen Version History.")?;
        let mut request=json!({"action":action,"id":state.selected,"label":state.label,"document":owner.document,"revision":owner.revision,"allowDiscard":action=="restore" && state.confirm});
        if !state.password.is_empty() {request["historyPassword"]=json!(state.password.as_str());}
        if action=="restore" {self.history_owner=None;}
        self.execute("file.history",request)
    }
}
fn reload(app:&mut WordApp,state:&mut HistoryDialog) {
    match app.history_command(state,"list") {
        Ok(value)=>{state.rows=value["versions"].as_array().cloned().unwrap_or_default();state.protected=value["protected"].as_bool();state.selected.clear();state.preview.clear();state.error.clear();state.confirm=false;}
        Err(error)=>{state.error=error;state.protected=None;state.rows.clear();state.selected.clear();state.preview.clear();state.confirm=false;},
    }
}
pub(crate) fn retire_if_detached(app:&mut WordApp) {if !matches!(app.dialog,Some(Dialog::VersionHistory {..})){app.history_owner=None;}}
pub(crate) fn body(app:&mut WordApp,ui:&mut egui::Ui,state:&mut HistoryDialog)->bool {
    let current=app.history_owner.as_ref().is_some_and(|owner|owner.matches(app));
    let mut close=false;
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min),|ui| {
        // Keep the captured decision controls outside the scrolling history content.
        ui.horizontal(|ui| {
            if current && state.confirm {
                if ui.button("Cancel restoration").clicked(){state.confirm=false;}
                if ui.button("Restore Version").clicked(){match app.history_command(state,"restore"){Ok(_)=>close=true,Err(error)=>{state.error=error;state.confirm=false;}}}
            } else {
                if ui.button("Close").clicked(){close=true;}
                if current && ui.add_enabled(!state.selected.is_empty(),egui::Button::new("Restore selected version…")).clicked(){state.confirm=true;}
            }
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical().id_salt("persistent-history-content").auto_shrink([false,false]).show(ui,|ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Min),|ui| {contents(app,ui,state,current);});
        });
    });close
}
fn contents(app:&mut WordApp,ui:&mut egui::Ui,state:&mut HistoryDialog,current:bool) {
    ui.label("Versions are stored locally beside this saved document. Keep its history folder with your backups. Moving or renaming the document does not move its history.");
    ui.label("Explicit captures only: up to 20 versions and 32 MiB per document. Oldest versions expire when these limits are reached. This is not an automatic backup.");
    ui.label(match state.protected {Some(true)=>"History archive is password protected.",Some(false)=>"History archive is not password protected. Earlier unprotected history is encrypted only when a new version is captured with a document password.",None=>"History protection could not be checked. Refresh with its history password when needed."});
    ui.label("History can use an earlier password after the document password changes. Enter that history password below when needed. Capturing a version preserves existing protection or uses the current document password.");
    if !state.error.is_empty(){ui.label(&state.error);}
    if !current {ui.label("This history request is no longer current. Close and reopen Version History.");return;}
    if state.confirm {ui.label("Restore this version into the captured working document? This replaces unsaved changes. A Before restore version is captured first. Your saved source file remains unchanged until you explicitly save.");return;}
    ui.horizontal(|ui|{let label=ui.label("History password:");ui.add(egui::TextEdit::singleline(state.password.as_mut_string()).password(true).desired_width(180.0)).labelled_by(label.id);if ui.button("Refresh history").clicked(){reload(app,state);}});
    ui.horizontal(|ui|{let label=ui.label("Version label:");ui.text_edit_singleline(&mut state.label).labelled_by(label.id);if ui.button("Capture version").clicked(){match app.history_command(state,"capture"){Ok(_)=>reload(app,state),Err(error)=>state.error=error,}}});
    let mut selected=None;
    egui::ScrollArea::vertical().id_salt("persistent-version-list").max_height(150.0).show(ui,|ui|{
        for row in &state.rows {let id=row["id"].as_str().unwrap_or("");let text=format!("{} · {} · {} words",row["label"].as_str().unwrap_or("Version"),row["date"].as_str().unwrap_or(""),row["words"]);if ui.selectable_label(state.selected==id,text).clicked(){selected=Some(id.to_owned());}}
    });
    if state.rows.is_empty(){ui.label("No persistent versions have been captured for this saved document.");}
    if let Some(id)=selected {state.selected=id;match app.history_command(state,"preview"){Ok(value)=>{state.preview=value["text"].as_str().unwrap_or("").into();state.error.clear();},Err(error)=>state.error=error,}}
    if !state.preview.is_empty(){egui::ScrollArea::vertical().id_salt("persistent-version-preview").max_height(150.0).show(ui,|ui|{ui.label(&state.preview);});}
}

#[cfg(test)]
mod persistent_history_ui_tests {
    use super::*;
    use crate::Services;
    use wordcraft_doc::{Document,StoryRef};
    use wordcraft_engine::Session;
    use std::sync::atomic::{AtomicU64,Ordering};
    static NEXT:AtomicU64=AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new()->Self{let path=std::env::temp_dir().join(format!("history-ui-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));std::fs::create_dir(&path).unwrap();Self(path)}
        fn app(&self)->WordApp{let mut session=Session::new(Document::from_text("Original working document"));session.run("file.save",&json!({"path":self.0.join("document.docx")})).unwrap();WordApp::new(session,Services::default())}
    }
    impl Drop for Fixture{fn drop(&mut self){std::fs::remove_dir_all(&self.0).unwrap();}}
    fn dialog_state(app:&WordApp)->HistoryDialog{match app.dialog.as_ref(){Some(Dialog::VersionHistory {state})=>state.clone(),_=>panic!("Expected owned history dialog")}}
    #[test]
    fn persistent_history_ui_captured_owner_refuses_edit_and_replacement(){
        let root=Fixture::new();let mut app=root.app();app.open_history();let state=dialog_state(&app);app.session.run("text.insert",&json!({"text":"Newer unsaved "})).unwrap();let before=app.session.doc.plain_text(StoryRef::Body);assert!(app.history_command(&state,"capture").is_err());assert_eq!(app.session.doc.plain_text(StoryRef::Body),before);
        app.dialog=None;retire_if_detached(&mut app);assert!(app.history_owner.is_none());app.open_history();let state=dialog_state(&app);app.session.run("file.new",&json!({})).unwrap();assert!(app.history_command(&state,"capture").is_err());
    }
    #[test]
    fn persistent_history_ui_restore_requires_decision_and_is_one_shot(){
        let root=Fixture::new();let mut app=root.app();app.open_history();let mut state=dialog_state(&app);app.history_command(&state,"capture").unwrap();reload(&mut app,&mut state);state.selected=state.rows[0]["id"].as_str().unwrap().into();app.dialog=None;app.history_owner=None;
        app.session.run("text.insert",&json!({"text":"Unsaved edit "})).unwrap();app.open_history();let mut state=dialog_state(&app);state.selected=app.session.run("file.history",&json!({})).unwrap()["versions"][0]["id"].as_str().unwrap().into();let before=app.session.doc.plain_text(StoryRef::Body);assert!(app.history_command(&state,"restore").is_err());assert_eq!(app.session.doc.plain_text(StoryRef::Body),before);
        app.dialog=None;app.open_history();let mut state=dialog_state(&app);state.selected=app.session.run("file.history",&json!({})).unwrap()["versions"][0]["id"].as_str().unwrap().into();state.confirm=true;app.history_command(&state,"restore").unwrap();assert!(app.session.dirty);assert_eq!(app.session.doc.plain_text(StoryRef::Body),"Original working document");assert!(app.history_command(&state,"restore").is_err());
    }
    #[test]
    fn persistent_history_ui_failed_protected_read_never_claims_plain_history(){
        let root=Fixture::new();let mut app=root.app();app.session.run("file.encrypt",&json!({"password":"Synthetic history password"})).unwrap();app.session.run("file.history",&json!({"action":"capture"})).unwrap();app.session.password=None;app.open_history();let state=dialog_state(&app);assert!(state.protected.is_none());assert!(state.error.contains("capture-time password"));
    }
    #[test]
    fn persistent_history_ui_real_browse_capture_cancel_and_restore_widgets(){
        use egui_kittest::kittest::Queryable;
        for size in [egui::vec2(1100.0,900.0),egui::vec2(700.0,480.0)] {
        let root=Fixture::new();let mut app=root.app();app.run("file.history",json!({})).unwrap();assert!(matches!(app.dialog,Some(Dialog::VersionHistory {..})));
        let mut harness=egui_kittest::Harness::builder().with_size(size).build_ui_state(|ui,app:&mut WordApp|{let ctx=ui.ctx().clone();app.logic(&ctx);app.ui(ui);},app);
        harness.run_steps(4);harness.get_by_label("Capture version").click();harness.run_steps(4);let state=dialog_state(harness.state());assert_eq!(state.rows.len(),1);let row=&state.rows[0];let label=format!("{} · {} · {} words",row["label"].as_str().unwrap(),row["date"].as_str().unwrap(),row["words"]);harness.get_by_label(&label).click();harness.run_steps(4);assert_eq!(dialog_state(harness.state()).preview,"Original working document");
        harness.get_by_label("Close").click();harness.run_steps(4);assert!(harness.state().dialog.is_none());harness.state_mut().session.run("text.insert",&json!({"text":"Newer unsaved "})).unwrap();let edited=harness.state().session.doc.plain_text(StoryRef::Body);harness.state_mut().open_history();harness.run_steps(4);harness.get_by_label(&label).click();harness.run_steps(4);harness.get_by_label("Restore selected version…").click();harness.run_steps(4);assert!(dialog_state(harness.state()).confirm);harness.get_by_label("Cancel restoration").click();harness.run_steps(4);assert_eq!(harness.state().session.doc.plain_text(StoryRef::Body),edited);
        harness.get_by_label("Restore selected version…").click();harness.run_steps(4);harness.get_by_label("Restore Version").click();harness.run_steps(4);assert!(harness.state().dialog.is_none());assert_eq!(harness.state().session.doc.plain_text(StoryRef::Body),"Original working document");assert!(harness.state().session.dirty);
        }
    }
    #[test]
    fn persistent_history_ui_footer_stays_fixed_for_long_recovery_preview(){
        use egui_kittest::kittest::Queryable;
        for size in [egui::vec2(1100.0,900.0),egui::vec2(700.0,480.0)] {
            let root=Fixture::new();let mut app=root.app();app.open_history();
            let mut harness=egui_kittest::Harness::builder().with_size(size).build_ui_state(|ui,app:&mut WordApp|{let ctx=ui.ctx().clone();app.logic(&ctx);app.ui(ui);},app);
            harness.run_steps(4);let initial=harness.get_by_label("Close").rect();
            match harness.state_mut().dialog.as_mut().unwrap(){Dialog::VersionHistory {state}=>{
                state.rows=(0..20).map(|index|json!({"id":format!("{index:x}"),"label":"Retained recovery snapshot","date":"2026-01-01T00:00:00Z","words":500})).collect();state.selected="0".into();state.preview="A long retained recovery preview.\n".repeat(80);
            },_=>panic!("Expected owned history dialog")}
            harness.run_steps(4);let close=harness.get_by_label("Close").rect();let restore=harness.get_by_label("Restore selected version…").rect();
            assert_eq!(initial,close,"Preview growth must not displace the decision footer");
            let viewport=egui::Rect::from_min_size(egui::Pos2::ZERO,size);assert!(viewport.contains_rect(close));assert!(viewport.contains_rect(restore));
            harness.get_by_label("Restore selected version…").click();harness.run_steps(4);
            assert!(viewport.contains_rect(harness.get_by_label("Cancel restoration").rect()));assert!(viewport.contains_rect(harness.get_by_label("Restore Version").rect()));
            harness.get_by_label("Cancel restoration").click();harness.run_steps(4);harness.get_by_label("Close").click();harness.run_steps(4);assert!(harness.state().dialog.is_none());
        }
    }
    #[test]
    fn persistent_history_ui_unsaved_refusal_and_secret_is_never_serialized(){
        let mut app=WordApp::new(Session::new(Document::new()),Services::default());app.open_history();let mut state=dialog_state(&app);assert!(state.error.contains("save the document"));state.password=Password::new("Synthetic history-only credential");let encoded=serde_json::to_string(&state).unwrap();assert!(!encoded.contains("Synthetic history-only credential"));assert!(!encoded.contains("document_id"));
    }
}
