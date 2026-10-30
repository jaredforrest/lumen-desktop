//! Physical workbook/presentation history decisions in owned native processes.
use super::*;
const WORKING: &str = "Newer unsaved history working marker";
const LATER: &str = "Newer unsaved history working marker Revision after held restore";
impl Native {
    fn provider_history_command(&mut self, name: &str, p: Value) -> Result<Value> {
        self.control
            .as_mut()
            .ok_or("Private control unavailable")?
            .command(&self.options.app, name, Some(p))
    }
    fn provider_history_state(&mut self) -> Result<Value> {
        let ui = self.app_ui()?;
        Ok(if self.options.app == "sheet" {
            ui["dialog"]["history"].clone()
        } else {
            ui["history"].clone()
        })
    }
    fn provider_history_content(&mut self) -> Result<Value> {
        live::snapshot(
            self.control.as_mut().ok_or("Private control unavailable")?,
            &self.options.app,
        )
    }
    fn provider_history_same(&mut self, expected: &Value, source_hash: &str) -> Result<()> {
        ensure(
            self.provider_history_content()? == *expected && sha(&self.source)? == source_hash,
            "History decision changed supported working projection or saved source",
        )
    }
    fn provider_history_edit(&mut self, text: &str) -> Result<()> {
        if self.options.app == "sheet" {
            self.provider_history_command("cell.set", json!({"cell":"A1","input":text}))?;
        } else {
            self.provider_history_command("slide.notes", json!({"index":0,"text":text}))?;
        }
        Ok(())
    }
    fn provider_history_click(&mut self, name: &str, button: bool, events: &[&str]) -> Result<()> {
        let is_history = !self.provider_history_state()?.is_null();
        let row = self.copy_button(name)?;
        ensure(
            !button
                || ["button", "push button", "role:43"]
                    .contains(&row["role"].as_str().unwrap_or("")),
            "Captured history action is not a button",
        )?;
        let a11y = self
            .a11y
            .as_ref()
            .ok_or("Private accessibility unavailable")?;
        let bounds = a11y.bounds_captured(&row)?;
        ensure(
            bounds.0 >= 0
                && bounds.1 >= 0
                && bounds.0 + bounds.2 <= number(&self.window["rect"]["width"])? as i32 + 1
                && bounds.1 + bounds.3 <= number(&self.window["rect"]["height"])? as i32 + 1,
            "History action falls outside its captured native window",
        )?;
        let frame = a11y
            .find_roles(self.pid as u32, "Version History", true, &["frame","role:23"])?;
        ensure(!is_history || frame.is_some(), "Visible history dialog has no captured owned frame")?;
        let dialog = frame
            .as_ref()
            .map(|frame| a11y.bounds_captured(frame))
            .transpose()?;
        if let Some(frame) = dialog {
            ensure(
                bounds.0 >= frame.0
                    && bounds.1 >= frame.1
                    && bounds.0 + bounds.2 <= frame.0 + frame.2 + 1
                    && bounds.1 + bounds.3 <= frame.1 + frame.3 + 1,
                "History action is clipped outside its painted dialog",
            )?;
        }
        let x = number(&self.window["rect"]["x"])? + number(&self.window["window_rect"]["x"])?;
        let y = number(&self.window["rect"]["y"])? + number(&self.window["window_rect"]["y"])?;
        self.set_fact(
            "providerHistoryLastControl",
            json!({"row":row,"bounds":bounds,"origin":[x,y],"dialog":dialog}),
        );
        self.pointer_events(
            x + bounds.0 as f64 + bounds.2 as f64 / 2.,
            y + bounds.1 as f64 + bounds.3 as f64 / 2.,
            events,
        )
    }
    fn provider_history_open(&mut self) -> Result<()> {
        self.provider_history_click("File", false, &["p", "r"])?;
        self.provider_history_click("Version History…", true, &["p", "r"])?;
        wait("actual provider File history route", || {
            let ui = self.app_ui()?;
            Ok((if self.options.app == "sheet" {
                ui["dialog"]["name"] == "history"
            } else {
                ui["dialog"] == "history"
            })
            .then_some(()))
        })
    }
    fn provider_history_select(&mut self, row: &Value) -> Result<()> {
        let id = row["id"].as_str().ok_or("History stable identity")?;
        let label = format!(
            "{} · {} · {}",
            row["label"].as_str().ok_or("History label")?,
            row["date"].as_str().ok_or("History date")?,
            row["summary"].as_str().ok_or("History summary")?
        );
        self.provider_history_click(&label, false, &["p", "r"])?;
        wait("actual provider stable version selected", || {
            Ok((self.provider_history_state()?["selected"] == id).then_some(()))
        })
    }
    fn provider_history_close(&mut self) -> Result<()> {
        self.provider_history_click("Close", true, &["p", "r"])?;
        wait("physical provider history Close", || {
            Ok(self.app_ui()?["dialog"].is_null().then_some(()))
        })
    }
    fn provider_history_confirm(&mut self) -> Result<()> {
        self.provider_history_click("Restore selected version…", true, &["p", "r"])?;
        wait("actual provider restore confirmation", || {
            Ok((self.provider_history_state()?["confirm"] == true).then_some(()))
        })
    }
    fn provider_history_release(&mut self) -> Result<()> {
        let input = self.processes[self.pointer.ok_or("Pointer unavailable")?]
            .0
            .stdin
            .as_mut()
            .ok_or("Pointer input unavailable")?;
        writeln!(input, "r")?;
        input.flush()?;
        Ok(())
    }
    fn provider_history_recovery(&mut self) -> Result<()> {
        let state = self.provider_history_state()?;
        let text = state["preview"]
            .as_str()
            .ok_or("Recovery preview missing")?;
        ensure(
            text.contains(WORKING) && text.contains("Revision after held restore"),
            "Recovery browser lost replaced working content",
        )
    }
    pub(super) fn provider_history(&mut self) -> Result<()> {
        ensure(
            matches!(self.options.app.as_str(), "sheet" | "slides"),
            "Provider history requires workbook or presentation",
        )?;
        self.pointer_start()?;
        let source_hash = sha(&self.source)?;
        let original = self.provider_history_content()?;
        self.provider_history_open()?;
        self.provider_history_click("Capture version", true, &["p", "r"])?;
        let saved = wait("physically captured provider version", || {
            Ok(self.provider_history_state()?["rows"]
                .as_array()
                .and_then(|rows| rows.first())
                .cloned())
        })?;
        ensure(
            saved["label"] == "Saved version",
            "Physical provider capture label differs",
        )?;
        self.provider_history_same(&original, &source_hash)?;
        self.provider_history_close()?;
        self.checks.push("Actual File menu history route and physical Capture preserve supported working projection and source bytes".into());
        self.provider_history_edit(WORKING)?;
        let edited = self.provider_history_content()?;
        ensure(
            self.control_command("document.inspect")?["dirty"] == true,
            "History working edit must be dirty",
        )?;
        self.provider_history_open()?;
        self.provider_history_select(&saved)?;
        let preview = self.provider_history_command(
            "file.history",
            json!({"action":"preview","id":saved["id"]}),
        )?;
        ensure(
            self.provider_history_state()?["preview"] == preview["text"],
            "Visible provider preview differs from selected stable version",
        )?;
        self.screenshot("provider-history-selected.png")?;
        self.provider_history_confirm()?;
        self.provider_history_click("Cancel restoration", true, &["p", "r"])?;
        wait("physical provider restore Cancel", || {
            Ok((self.provider_history_state()?["confirm"] == false).then_some(()))
        })?;
        self.provider_history_same(&edited, &source_hash)?;
        self.checks.push("Physical stable-row selection previews the retained version and Cancel preserves unsaved working content".into());
        self.provider_history_confirm()?;
        self.provider_history_click("Restore Version", true, &["p"])?;
        self.provider_history_edit(LATER)?;
        let newer = self.provider_history_content()?;
        wait("stale provider owner removes held restore action", || {
            Ok(self
                .a11y
                .as_ref()
                .ok_or("Private accessibility unavailable")?
                .find(self.pid as u32, "Restore Version", true)?
                .is_none()
                .then_some(()))
        })?;
        self.provider_history_release()?;
        self.provider_history_same(&newer, &source_hash)?;
        self.provider_history_close()?;
        self.checks.push(
            "Held physical Restore cannot dispatch after the captured provider revision changes"
                .into(),
        );
        self.provider_history_open()?;
        self.provider_history_select(&saved)?;
        self.provider_history_confirm()?;
        self.provider_history_click("Restore Version", true, &["p", "r"])?;
        wait("explicit physical provider Restore completes", || {
            Ok(self.app_ui()?["dialog"].is_null().then_some(()))
        })?;
        self.provider_history_same(&original, &source_hash)?;
        ensure(
            self.control_command("document.inspect")?["dirty"] == true,
            "Restored provider state must remain unsaved",
        )?;
        let versions =
            self.provider_history_command("file.history", json!({"action":"list"}))?["versions"]
                .clone();
        ensure(
            versions[0]["label"] == "Before restore",
            "Provider Before restore snapshot missing",
        )?;
        self.checks.push("Fresh physical Restore replaces captured working content, preserves source and publishes Before restore recovery".into());
        self.provider_history_open()?;
        self.provider_history_select(&versions[0])?;
        self.provider_history_recovery()?;
        self.provider_history_close()?;
        self.checks.push("The actual provider browser exposes the pre-restore unsaved content with visible decision controls".into());
        self.provider_history_reopen(&original, &source_hash, &saved, &versions[0])?;
        self.set_fact("providerHistoryScope",json!("Physical owned controls and independent native process reopen for the existing disposable corpus. Full embedded payload representation is separately covered by provider unit tests; no external cloud or visual reference parity claim."));
        Ok(())
    }
    fn provider_history_reopen(
        &mut self,
        original: &Value,
        source_hash: &str,
        saved: &Value,
        recovery: &Value,
    ) -> Result<()> {
        let previous = self.facts["native"].clone();
        let wrapper = previous["profileWrapperPid"]
            .as_u64()
            .ok_or("Captured wrapper PID")? as u32;
        let index = self
            .processes
            .iter()
            .position(|child| child.0.id() == wrapper)
            .ok_or("Captured provider process missing")?;
        self.kill_window()?;
        wait("actual dirty native provider close asks", || {
            let ui = self.app_ui()?;
            Ok((if self.options.app == "sheet" {
                ui["dialog"]["name"] == "saveChanges"
            } else {
                ui["dialog"] == "quit"
            })
            .then_some(()))
        })?;
        self.provider_history_click("Don't Save", true, &["p", "r"])?;
        let title = self.source.file_name().and_then(|s| s.to_str()).ok_or("Captured source title")?.to_owned();
        let discarded = wait("physical provider discard releases captured document", || {
            if let Some(status) = self.processes[index].0.try_wait()? {
                ensure(status.success(), "Provider failed during physical document discard")?;
                return Ok(Some(json!({"processExited":true,"status":status.code()})));
            }
            let ui = self.app_ui()?;
            let docs = ui["documents"].as_array().ok_or("Provider document list unavailable after discard")?;
            let retained = docs.iter().any(|d| d["title"] == title || d["dirty"] == true);
            let active = if docs.is_empty() { Value::Null } else { self.control_command("document.inspect")? };
            fs::write(self.out("provider-history-discard-observation.json"), serde_json::to_vec_pretty(&json!({"ui":ui,"active":active}))?)?;
            Ok((!retained && active["path"] != self.source.display().to_string()).then_some(json!({"processExited":false,"ui":ui,"active":active})))
        })?;
        self.set_fact("providerHistoryPhysicalDiscard", discarded);
        self.control.take();
        // Document close can retain the provider's empty application window.
        // Reopen proof deliberately retires this owned process group after a
        // verified physical discard; it does not claim application-close parity.
        self.processes[index].stop();
        ensure(self.processes[index].0.try_wait()?.is_some(), "Owned first provider was not reaped")?;
        wait("retired provider native window disappears", || Ok(self.native()?.is_none().then_some(())))?;
        ensure(
            sha(&self.source)? == source_hash,
            "Physical discard changed source bytes",
        )?;
        self.port = 34218;
        self.env.insert(
            format!("{}_CONTROL_PORT", self.options.name().to_uppercase()),
            self.port.to_string(),
        );
        let app = self.options.app_binary().display().to_string();
        let source = self.source.display().to_string();
        let reopened = self.launch(&app, &[&source], "provider-history-reopened", false)?;
        wait("fresh provider private listener", || {
            Ok(self.listener().then_some(()))
        })?;
        self.window = wait("fresh provider native identity", || {
            Ok(self
                .native()?
                .filter(|window| window["pid"] != previous["pid"]))
        })?;
        self.pid = self.window["pid"].as_i64().ok_or("Reopened native PID")?;
        ensure(
            self.processes[reopened].0.try_wait()?.is_none(),
            "Reopened provider wrapper exited",
        )?;
        self.control = Some(Control::start(
            &[
                self.options.cli().display().to_string(),
                "mcp".into(),
                "--connect".into(),
                self.port.to_string(),
            ],
            &self.out("provider-history-reopened-control.log"),
            Some(&self.env),
        )?);
        self.control
            .as_mut()
            .ok_or("Reopened controller unavailable")?
            .initialize()?;
        ensure(
            self.control_command("document.inspect")?["path"] == source,
            "Reopened controller is not bound to original source",
        )?;
        self.command(
            "swaymsg",
            &[&format!("[con_id={}]", self.window["id"]), "focus"],
        )?;
        self.provider_history_same(original, source_hash)?;
        self.provider_history_open()?;
        let rows = self.provider_history_state()?["rows"]
            .as_array()
            .ok_or("Reopened provider rows missing")?
            .clone();
        ensure(
            rows.iter().any(|r| r["id"] == saved["id"])
                && rows.iter().any(|r| r["id"] == recovery["id"]),
            "Provider reopen lost stable retained identities",
        )?;
        self.provider_history_select(
            rows.iter()
                .find(|r| r["id"] == recovery["id"])
                .ok_or("Reopened provider recovery missing")?,
        )?;
        self.provider_history_recovery()?;
        self.screenshot("provider-history-process-reopened.png")?;
        self.provider_history_close()?;
        self.provider_history_same(original, source_hash)?;
        self.set_fact("providerHistoryReopen",json!({"previous":previous,"current":{"conId":self.window["id"],"pid":self.pid,"profileWrapperPid":self.processes[reopened].0.id(),"controlPort":self.port,"source":self.source},"persisted":rows,"controlFrames":self.control.as_ref().ok_or("Reopened controller unavailable")?.frames}));
        self.checks.push("After verified physical document discard and owned process shutdown, an independent native provider reopens source and browses retained captured and recovery versions".into());
        Ok(())
    }
}
