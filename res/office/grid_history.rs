//! Full semantic workbook snapshots; formulas and derived caches are rebuilt on restore.
use crate::{EngineError, Result, Selection, Session, UiRequest};
use gridcraft_model::Workbook;
use lumen_document_history::Store;
use serde_json::{Value, json};
fn failed(error: impl std::fmt::Display) -> EngineError {
    EngineError::Other(format!("Workbook history: {error}"))
}
fn summary(wb: &Workbook) -> String {
    format!("{} sheets", wb.sheets.len())
}
fn decode(value: Value) -> Result<Workbook> {
    let mut wb: Workbook = serde_json::from_value(value).map_err(failed)?;
    if wb.sheets.is_empty() || wb.active_sheet >= wb.sheets.len() {
        return Err(failed("version workbook has no valid active sheet"));
    }
    wb.styles.rebuild_index();
    Ok(wb)
}
pub(crate) fn command(session: &mut Session, p: &Value) -> Result<Value> {
    let action = p.get("action").and_then(Value::as_str).unwrap_or("list");
    let d = session.doc()?;
    if p.get("action").is_none() {
        let value = json!({"opened":true,"document":d.uid,"revision":d.revision});
        session
            .ui_requests
            .push(UiRequest::Dialog("history".into(), json!({})));
        return Ok(value);
    }
    if p.get("document")
        .and_then(Value::as_u64)
        .is_some_and(|uid| uid != d.uid)
        || p.get("revision")
            .and_then(Value::as_u64)
            .is_some_and(|rev| rev != d.revision)
    {
        return Err(failed("captured workbook or revision changed"));
    }
    if action == "restore"
        && (p.get("document").and_then(Value::as_u64) != Some(d.uid)
            || p.get("revision").and_then(Value::as_u64) != Some(d.revision))
    {
        return Err(failed(
            "restore requires the captured workbook and revision",
        ));
    }
    if !matches!(action, "list" | "capture" | "preview" | "restore") {
        return Err(failed("unknown history action"));
    }
    let path = d
        .path
        .as_deref()
        .ok_or_else(|| failed("save the workbook before using persistent history"))?;
    let key = p
        .get("historyPassword")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty());
    let mut store = Store::open(
        std::path::Path::new(path),
        "gridcraft",
        key,
        action == "capture",
    )
    .map_err(failed)?;
    let uid = d.uid;
    let revision = d.revision;
    match action {
        "list" => Ok(
            json!({"versions":store.as_ref().map(Store::rows).unwrap_or_else(||json!([])),"protected":store.as_ref().is_some_and(Store::protected),"document":uid,"revision":revision,"limit":20}),
        ),
        "capture" => {
            let store = store
                .as_mut()
                .ok_or_else(|| failed("history store unavailable"))?;
            store
                .capture(
                    serde_json::to_value(&*d.wb).map_err(failed)?,
                    p.get("label")
                        .and_then(Value::as_str)
                        .unwrap_or("Saved version"),
                    &summary(&d.wb),
                    key,
                )
                .map_err(failed)?;
            Ok(
                json!({"versions":store.rows(),"protected":store.protected(),"document":uid,"revision":revision,"limit":20}),
            )
        }
        "preview" | "restore" => {
            let store = store
                .as_mut()
                .ok_or_else(|| failed("no persistent versions have been captured"))?;
            let document = decode(
                store
                    .version(
                        p.get("id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| failed("select a stable version"))?,
                    )
                    .map_err(failed)?,
            )?;
            if action == "preview" {
                let mut text = String::new();
                for sheet in &document.sheets {
                    if text.chars().count() >= 2000 {
                        break;
                    }
                    text.push_str(&format!("{}\n", sheet.name));
                    for (cell, value) in sheet.cells.iter().take(20) {
                        text.push_str(&format!("{}: {}\n", cell.a1(), value.value.display()));
                        if text.chars().count() >= 2000 {
                            break;
                        }
                    }
                }
                return Ok(
                    json!({"text":text.chars().take(2000).collect::<String>(),"sheets":document.sheets.len()}),
                );
            }
            if d.is_dirty() && p.get("allowDiscard").and_then(Value::as_bool) != Some(true) {
                return Err(failed(
                    "explicit confirmation is required to replace unsaved changes",
                ));
            }
            store
                .capture(
                    serde_json::to_value(&*d.wb).map_err(failed)?,
                    "Before restore",
                    &summary(&d.wb),
                    key,
                )
                .map_err(failed)?;
            let mut document = document;
            let mut calc = gridcraft_calc::Calc::new();
            calc.recalc_all(&mut document);
            let selection = Selection::at(
                document
                    .active()
                    .ok_or_else(|| failed("version active sheet is missing"))?
                    .view_active,
            );
            let d = session.doc_mut()?;
            d.wb = std::sync::Arc::new(document);
            d.calc = calc;
            d.selection = selection;
            d.sheet_selections.clear();
            Ok(json!({"restored":true,"dirty":true,"formulasRecalculated":true}))
        }
        _ => Err(failed("unknown history action")),
    }
}

#[cfg(test)]
mod persistent_history_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "grid-history-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn session(&self) -> Session {
            let mut s = Session::new();
            s.new_workbook();
            s.execute("cell.set", json!({"cell":"A1","input":"5"}))
                .unwrap();
            s.execute("cell.set", json!({"cell":"B1","input":"=A1*2"}))
                .unwrap();
            s.execute("file.save", json!({"path":self.0.join("source.json")}))
                .unwrap();
            s
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn facts(s: &Session) -> Value {
        let d = s.doc().unwrap();
        json!({"workbook":&*d.wb,"uid":d.uid,"revision":d.revision,"path":d.path,"dirty":d.is_dirty(),"selection":d.selection})
    }
    fn restore(s: &mut Session, id: &str, allow: bool, key: &str) -> Result<Value> {
        let d = s.doc()?;
        let request = json!({"action":"restore","id":id,"document":d.uid,"revision":d.revision,"allowDiscard":allow,"historyPassword":key});
        s.execute("file.history", request)
    }
    #[test]
    fn persistent_history_reopen_consent_formula_rebuild_restore_and_undo() {
        let root = Fixture::new();
        let mut s = root.session();
        let path = root.0.join("source.json");
        let bytes = std::fs::read(&path).unwrap();
        let original = facts(&s);
        let captured = s
            .execute(
                "file.history",
                json!({"action":"capture","label":"Original"}),
            )
            .unwrap();
        let id = captured["versions"][0]["id"].as_str().unwrap();
        assert_eq!(facts(&s), original);
        let mut reopened = Session::new();
        reopened.execute("file.open", json!({"path":path})).unwrap();
        assert_eq!(
            reopened
                .execute("file.history", json!({"action":"list"}))
                .unwrap()["versions"][0]["id"],
            id
        );
        s.execute("cell.set", json!({"cell":"A1","input":"23"}))
            .unwrap();
        let edited = facts(&s);
        assert!(restore(&mut s, id, false, "").is_err());
        assert_eq!(facts(&s), edited);
        restore(&mut s, id, true, "").unwrap();
        assert_eq!(facts(&s)["workbook"], original["workbook"]);
        assert!(s.doc().unwrap().is_dirty());
        let rows = s.execute("file.history", json!({"action":"list"})).unwrap();
        assert_eq!(rows["versions"][0]["label"], "Before restore");
        let preview = s
            .execute(
                "file.history",
                json!({"action":"preview","id":rows["versions"][0]["id"]}),
            )
            .unwrap();
        assert!(preview["text"].as_str().unwrap().contains("23"));
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(facts(&s)["workbook"], edited["workbook"]);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    #[test]
    fn persistent_history_protection_invalid_stale_and_secret_journal() {
        let root = Fixture::new();
        let mut s = root.session();
        let journal = s.journal.len();
        let key = "Disposable workbook archive password";
        let captured = s
            .execute(
                "file.history",
                json!({"action":"capture","historyPassword":key}),
            )
            .unwrap();
        let id = captured["versions"][0]["id"].as_str().unwrap();
        assert_eq!(s.journal.len(), journal);
        assert_eq!(captured["protected"], true);
        let before = facts(&s);
        assert!(s.execute("file.history", json!({"action":"list"})).is_err());
        assert!(restore(&mut s, "invalid", true, key).is_err());
        assert_eq!(facts(&s), before);
        let d = s.doc().unwrap();
        let stale = json!({"action":"restore","id":id,"document":d.uid,"revision":d.revision,"allowDiscard":true,"historyPassword":key});
        s.execute("cell.set", json!({"cell":"A1","input":"42"}))
            .unwrap();
        let edited = facts(&s);
        assert!(s.execute("file.history", stale).is_err());
        assert_eq!(facts(&s), edited);
    }
    #[test]
    fn persistent_history_semantic_snapshot_preserves_binary_images() {
        let mut wb = Workbook::new();
        let sheet = std::sync::Arc::make_mut(&mut wb.sheets[0]);
        sheet.images.push(gridcraft_model::Image {
            id: 1,
            anchor: Default::default(),
            data: vec![0, 255, 13, 128],
            mime: "image/png".into(),
            alt: "Binary fixture".into(),
        });
        let snapshot = serde_json::to_value(&wb).unwrap();
        let decoded = decode(snapshot).unwrap();
        assert_eq!(decoded.sheets[0].images[0].data, [0, 255, 13, 128]);
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            serde_json::to_value(wb).unwrap()
        );
    }
}
