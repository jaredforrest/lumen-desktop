//! Physical captured history decisions and independent native process reopen.
use super::*;

impl Native {
    fn history_command(&mut self, name: &str, params: Value) -> Result<Value> {
        self.control.as_mut().ok_or("Control unavailable")?.command("writer", name, Some(params))
    }
    fn history_state(&mut self) -> Result<Value> { Ok(self.app_ui()?["dialog"]["state"].clone()) }
    fn history_content(&mut self) -> Result<Value> {
        live::snapshot(self.control.as_mut().ok_or("Control unavailable")?, "writer")
    }
    fn history_same(&mut self, expected: &Value, source_hash: &str) -> Result<()> {
        ensure(self.history_content()? == *expected && sha(&self.source)? == source_hash,
               "History decision changed complete working content or saved source")
    }
    fn history_click(&mut self, name: &str, button: bool, events: &[&str]) -> Result<()> {
        let row = self.copy_button(name)?;
        ensure(!button || ["button", "push button", "role:43"].contains(&row["role"].as_str().unwrap_or("")),
               "Captured history action is not a button")?;
        let a11y = self.a11y.as_ref().ok_or("Private accessibility unavailable")?;
        let bounds = a11y.bounds_captured(&row)?;
        ensure(bounds.0 >= 0 && bounds.1 >= 0 && bounds.0 + bounds.2 <= number(&self.window["rect"]["width"])? as i32 + 1 && bounds.1 + bounds.3 <= number(&self.window["rect"]["height"])? as i32 + 1,
               "History action falls outside its captured native window")?;
        let frame = a11y.find(self.pid as u32, "Version History", true)?;
        let dialog = frame.as_ref().map(|frame| a11y.bounds_captured(frame)).transpose()?;
        if let Some(frame) = dialog {
            ensure(bounds.0 >= frame.0 && bounds.1 >= frame.1 && bounds.0 + bounds.2 <= frame.0 + frame.2 + 1 && bounds.1 + bounds.3 <= frame.1 + frame.3 + 1,
                   "History action is clipped outside its painted dialog")?;
        }
        let x = number(&self.window["rect"]["x"])? + number(&self.window["window_rect"]["x"])?;
        let y = number(&self.window["rect"]["y"])? + number(&self.window["window_rect"]["y"])?;
        self.set_fact("historyLastControl", json!({"row":row,"bounds":bounds,"origin":[x,y],"dialog":dialog}));
        self.pointer_events(x + bounds.0 as f64 + bounds.2 as f64 / 2., y + bounds.1 as f64 + bounds.3 as f64 / 2., events)
    }
    fn history_open(&mut self) -> Result<()> {
        self.history_command("ui.backstage", json!({"value":true,"page":"info"}))?;
        self.history_click("Version History…", true, &["p", "r"])?;
        wait("actual document history dialog", || Ok((self.app_ui()?["dialog"]["dialog"] == "versionHistory").then_some(())))
    }
    fn history_select(&mut self, row: &Value) -> Result<()> {
        let id = row["id"].as_str().ok_or("History version identity")?;
        let label = format!("{} · {} · {} words", row["label"].as_str().ok_or("History label")?, row["date"].as_str().ok_or("History date")?, row["words"]);
        self.history_click(&label, false, &["p", "r"])?;
        wait("exact stable version selection", || Ok((self.history_state()?["selected"] == id).then_some(())))
    }
    fn history_close(&mut self) -> Result<()> {
        self.history_click("Close", true, &["p", "r"])?;
        wait("physical history Close", || Ok(self.app_ui()?["dialog"].is_null().then_some(())))
    }
    fn history_confirm(&mut self) -> Result<()> {
        self.history_click("Restore selected version…", true, &["p", "r"])?;
        wait("actual captured restore confirmation", || Ok((self.history_state()?["confirm"] == true).then_some(())))
    }
    fn history_release_pointer(&mut self) -> Result<()> {
        let stdin = self.processes[self.pointer.ok_or("Pointer unavailable")?].0.stdin.as_mut().ok_or("Pointer input unavailable")?;
        writeln!(stdin, "r")?; stdin.flush()?; Ok(())
    }
    pub(super) fn history(&mut self) -> Result<()> {
        self.pointer_start()?;
        let source_hash = sha(&self.source)?;
        let original = self.history_content()?;
        self.history_open()?;
        self.history_click("Capture version", true, &["p", "r"])?;
        let saved = wait("physically captured stable version", || Ok(self.history_state()?["rows"].as_array().and_then(|rows| rows.first()).cloned()))?;
        ensure(saved["label"] == "Saved version", "Physical capture label differs")?;
        self.history_same(&original, &source_hash)?; self.history_close()?;
        self.checks.push("Actual Info history route and physical Capture preserve complete working projection and original source".into());
        self.history_command("caret.docEnd", json!({}))?;
        self.history_command("text.insert", json!({"text":" Newer unsaved history working marker"}))?;
        let edited = self.history_content()?;
        ensure(self.control_command("file.info")?["dirty"] == true, "History working edit must be dirty")?;
        self.history_open()?; self.history_select(&saved)?;
        let preview = self.history_command("file.history", json!({"action":"preview","id":saved["id"]}))?;
        ensure(self.history_state()?["preview"] == preview["text"], "Actual preview differs from selected stable version")?;
        self.screenshot("history-selected.png")?; self.history_confirm()?;
        self.history_click("Cancel restoration", true, &["p", "r"])?;
        wait("physical restoration Cancel", || Ok((self.history_state()?["confirm"] == false).then_some(())))?;
        self.history_same(&edited, &source_hash)?;
        self.checks.push("Physical stable-row selection previews content and Cancel preserves complete unsaved working document".into());
        self.history_confirm()?; self.history_click("Restore Version", true, &["p"])?;
        self.history_command("text.insert", json!({"text":" Revision after held restore"}))?;
        let newer = self.history_content()?;
        wait("stale owner removes held restore action", || Ok(self.a11y.as_ref().ok_or("Accessibility unavailable")?.find(self.pid as u32, "Restore Version", true)?.is_none().then_some(())))?;
        self.history_release_pointer()?; self.history_same(&newer, &source_hash)?; self.history_close()?;
        self.checks.push("Held physical Restore cannot dispatch after captured document revision changes".into());
        self.history_open()?; self.history_select(&saved)?; self.history_confirm()?;
        self.history_click("Restore Version", true, &["p", "r"])?;
        wait("explicit physical Restore completes", || Ok(self.app_ui()?["dialog"].is_null().then_some(())))?;
        self.history_same(&original, &source_hash)?;
        ensure(self.control_command("file.info")?["dirty"] == true, "Restored working content must remain dirty")?;
        let versions = self.history_command("file.history", json!({"action":"list"}))?["versions"].clone();
        ensure(versions[0]["label"] == "Before restore", "Pre-restore recovery missing")?;
        self.checks.push("Fresh physical Restore replaces only captured working content, preserves saved source and creates Before restore recovery".into());
        self.history_open()?; self.history_select(&versions[0])?;
        self.history_recovery_preview()?; self.history_close()?;
        self.checks.push("The actual version browser exposes complete pre-restore recovery content with visible decision controls".into());
        self.history_process_reopen(&original, &source_hash, &saved, &versions[0])
    }
    fn history_recovery_preview(&mut self) -> Result<()> {
        let state = self.history_state()?;
        let preview = state["preview"].as_str().ok_or("Recovery preview")?;
        ensure(preview.contains("Newer unsaved history working marker") && preview.contains("Revision after held restore"), "Recovery preview lost captured unsaved content")
    }
    fn history_process_reopen(&mut self, original: &Value, source_hash: &str, saved: &Value, recovery: &Value) -> Result<()> {
        let previous = self.facts["native"].clone();
        let wrapper = previous["profileWrapperPid"].as_u64().ok_or("Captured provider wrapper")? as u32;
        let index = self.processes.iter().position(|child| child.0.id() == wrapper).ok_or("Captured provider wrapper missing")?;
        self.key(&["-M", "ctrl", "-k", "w", "-m", "ctrl"])?;
        wait("actual dirty close asks before exit", || Ok((self.app_ui()?["dialog"]["dialog"] == "saveChanges").then_some(())))?;
        self.history_click("Don't Save", true, &["p", "r"])?;
        wait("physical explicit discard closes first provider", || Ok(self.processes[index].0.try_wait()?.map(|_| ())))?;
        self.control.take();
        ensure(sha(&self.source)? == source_hash, "Discard changed saved source")?;
        self.port = 34218;
        self.env.insert("WORDCRAFT_CONTROL_PORT".into(), self.port.to_string());
        let app = self.options.writer.display().to_string(); let source = self.source.display().to_string();
        let reopened = self.launch(&app, &[&source], "history-reopened", false)?;
        wait("fresh reopened private listener", || Ok(self.listener().then_some(())))?;
        self.window = wait("fresh native identity after process reopen", || Ok(self.native()?.filter(|window| window["pid"] != previous["pid"])))?;
        self.pid = self.window["pid"].as_i64().ok_or("Reopened native PID")?;
        ensure(self.processes[reopened].0.try_wait()?.is_none(), "Reopened provider wrapper exited")?;
        self.control = Some(Control::start(&[self.options.cli().display().to_string(), "mcp".into(), "--connect".into(), format!("127.0.0.1:{}", self.port)], &self.out("history-reopened-control.log"), Some(&self.env))?);
        self.control.as_mut().ok_or("Reopened control")?.initialize()?;
        ensure(self.control_command("file.info")?["path"] == source, "Reopened controller is not bound to source")?;
        self.command("swaymsg", &[&format!("[con_id={}]", self.window["id"]), "focus"])?;
        self.history_same(original, source_hash)?; self.history_open()?;
        let rows = self.history_state()?["rows"].as_array().ok_or("Reopened history rows")?.clone();
        ensure(rows.iter().any(|row| row["id"] == saved["id"]) && rows.iter().any(|row| row["id"] == recovery["id"]), "Process reopen lost stable captured versions")?;
        self.history_select(rows.iter().find(|row| row["id"] == recovery["id"]).ok_or("Reopened recovery version")?)?;
        self.history_recovery_preview()?; self.screenshot("history-process-reopened.png")?; self.history_close()?;
        self.set_fact("historyReopen", json!({"previous":previous,"current":{"conId":self.window["id"],"pid":self.pid,"profileWrapperPid":self.processes[reopened].0.id()},"persisted":rows}));
        self.checks.push("Independent native provider process reopens original source and browses persistent captured and pre-restore versions".into());
        self.history_same(original, source_hash)
    }
}
