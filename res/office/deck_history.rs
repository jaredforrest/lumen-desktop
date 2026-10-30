//! Durable local snapshots of the full semantic presentation.
use serde_json::{Value,json};
use crate::{EngineError,Result,Session,UiRequest};
use lumen_document_history::Store;
fn failed(error:impl std::fmt::Display)->EngineError{EngineError::Other(format!("Presentation history: {error}"))}
fn summary(doc:&deckcraft_model::Presentation)->String{format!("{} slides",doc.slides.len())}
fn snapshot(doc:&deckcraft_model::Presentation)->Result<Value>{Ok(json!({"presentation":serde_json::to_value(doc).map_err(failed)?,"mediaData":doc.media.iter().map(|media|crate::cmd::base64_encode(&media.data)).collect::<Vec<_>>()}))}
fn decode(value:Value)->Result<deckcraft_model::Presentation>{
    let mut doc:deckcraft_model::Presentation=serde_json::from_value(value.get("presentation").cloned().ok_or_else(||failed("version presentation is missing"))?).map_err(failed)?;
    let media=value.get("mediaData").and_then(Value::as_array).ok_or_else(||failed("version media is missing"))?;
    if media.len()!=doc.media.len(){return Err(failed("version media identities are inconsistent"));}
    for (item,data) in doc.media.iter_mut().zip(media){let encoded=data.as_str().ok_or_else(||failed("version media data is damaged"))?;let bytes=crate::cmd::base64_decode(encoded).ok_or_else(||failed("version media data is damaged"))?;if crate::cmd::base64_encode(&bytes)!=encoded{return Err(failed("version media data is not canonical"));}item.data=std::sync::Arc::new(bytes);}
    Ok(doc)
}
pub(crate) fn command(session:&mut Session,p:&Value)->Result<Value>{
    let action=p.get("action").and_then(Value::as_str).unwrap_or("list");
    let d=session.doc()?;
    if p.get("action").is_none(){let result=json!({"document":d.uid,"revision":d.revision,"opened":true});session.ui_requests.push(UiRequest::Dialog{id:"history".into(),params:Value::Null});return Ok(result);}
    if let Some(uid)=p.get("document").and_then(Value::as_u64){if uid!=d.uid{return Err(failed("captured presentation changed"));}}
    if let Some(revision)=p.get("revision").and_then(Value::as_u64){if revision!=d.revision{return Err(failed("captured presentation revision changed"));}}
    if action=="restore" && (p.get("document").and_then(Value::as_u64)!=Some(d.uid)||p.get("revision").and_then(Value::as_u64)!=Some(d.revision)){return Err(failed("restore requires the captured presentation and revision"));}
    if !matches!(action,"list"|"capture"|"preview"|"restore"){return Err(failed("unknown history action"));}
    let path=d.path.as_deref().ok_or_else(||failed("save the presentation before using persistent history"))?;
    let key=p.get("historyPassword").and_then(Value::as_str).filter(|s|!s.is_empty());
    let mut store=Store::open(std::path::Path::new(path),"deckcraft",key,action=="capture").map_err(failed)?;
    let uid=d.uid;let revision=d.revision;
    let result=match action {
        "list"=>json!({"versions":store.as_ref().map(Store::rows).unwrap_or_else(||json!([])),"protected":store.as_ref().is_some_and(Store::protected),"document":uid,"revision":revision,"limit":20}),
        "capture"=>{
            let store=store.as_mut().ok_or_else(||failed("history store is unavailable"))?;
            store.capture(snapshot(&d.doc)?,p.get("label").and_then(Value::as_str).unwrap_or("Saved version"),&summary(&d.doc),key).map_err(failed)?;
            json!({"versions":store.rows(),"protected":store.protected(),"document":uid,"revision":revision,"limit":20})
        },
        "preview"|"restore"=>{
            let store=store.as_mut().ok_or_else(||failed("no persistent versions have been captured"))?;
            let value=store.version(p.get("id").and_then(Value::as_str).ok_or_else(||failed("select a version"))?).map_err(failed)?;
            let document=decode(value)?;
            if action=="preview"{
                let mut text=deckcraft_format::slides_to_outline(&document);
                for slide in &document.slides{let notes=slide.notes.text();if !notes.is_empty(){text.push_str("\nNotes: ");text.push_str(&notes);}}
                json!({"text":text.chars().take(2000).collect::<String>(),"slides":document.slides.len()})
            }else{
                if d.is_dirty()&&!p.get("allowDiscard").and_then(Value::as_bool).unwrap_or(false){return Err(failed("explicit confirmation is required to replace unsaved changes"));}
                store.capture(snapshot(&d.doc)?,"Before restore",&summary(&d.doc),key).map_err(failed)?;
                let state=session.doc_mut()?;state.doc=std::sync::Arc::new(document);state.selection=Default::default();state.revision=state.revision.saturating_add(1);state.interaction=None;
                json!({"restored":true,"dirty":true})
            }
        },
        _=>return Err(failed("unknown history action")),
    };
    Ok(result)
}
#[cfg(test)]
mod persistent_history_tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64,Ordering};
    static NEXT:AtomicU64=AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture{
        fn new()->Self{let p=std::env::temp_dir().join(format!("deck-history-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));std::fs::create_dir(&p).unwrap();Self(p)}
        fn session(&self)->Session{let mut s=Session::with_new();s.execute("slide.notes",&json!({"index":0,"text":"Original speaker notes"})).unwrap();s.execute("file.saveAs",&json!({"path":self.0.join("source.deckcraft")})).unwrap();s}
    }
    impl Drop for Fixture{fn drop(&mut self){std::fs::remove_dir_all(&self.0).unwrap();}}
    fn facts(s:&Session)->Value{let d=s.active().unwrap();json!({"document":&*d.doc,"uid":d.uid,"revision":d.revision,"path":d.path,"dirty":d.is_dirty(),"selection":d.selection})}
    fn restore(s:&mut Session,id:&str,allow:bool,key:&str)->Result<Value>{let d=s.active().unwrap();let p=json!({"action":"restore","id":id,"document":d.uid,"revision":d.revision,"allowDiscard":allow,"historyPassword":key});s.execute("file.history",&p)}
    #[test]
    fn persistent_history_snapshot_keeps_embedded_bytes_and_rejects_missing_or_invalid_media(){
        let s=Session::with_new();let mut doc=(*s.active().unwrap().doc).clone();
        doc.media.push(deckcraft_model::MediaItem{name:"fixture.bin".into(),content_type:"application/octet-stream".into(),data:std::sync::Arc::new(vec![0,255,13,10,1,128]),..Default::default()});
        let value=snapshot(&doc).unwrap();let restored=decode(value.clone()).unwrap();assert_eq!(doc,restored);assert_eq!(restored.media[0].data.as_slice(),[0,255,13,10,1,128]);
        let mut missing=value.clone();missing["mediaData"]=json!([]);assert!(decode(missing).is_err());let mut invalid=value;invalid["mediaData"][0]=json!("invalid!");assert!(decode(invalid).is_err());
    }
    #[test]
    fn persistent_history_full_presentation_reopen_restore_consent_and_undo(){
        let root=Fixture::new();let mut s=root.session();let source=root.0.join("source.deckcraft");let bytes=std::fs::read(&source).unwrap();
        let original=serde_json::to_value(&*s.active().unwrap().doc).unwrap();let before=facts(&s);
        let captured=s.execute("file.history",&json!({"action":"capture","label":"Original"})).unwrap();let id=captured["versions"][0]["id"].as_str().unwrap();assert_eq!(facts(&s),before);
        let mut reopened=Session::with_new();reopened.execute("file.open",&json!({"path":source})).unwrap();let list=reopened.execute("file.history",&json!({"action":"list"})).unwrap();assert_eq!(list["versions"][0]["id"],id);
        s.execute("slide.notes",&json!({"index":0,"text":"Later unsaved speaker notes"})).unwrap();let edited=facts(&s);assert!(restore(&mut s,id,false,"").is_err());assert_eq!(facts(&s),edited);
        restore(&mut s,id,true,"").unwrap();assert_eq!(serde_json::to_value(&*s.active().unwrap().doc).unwrap(),original);assert!(s.active().unwrap().is_dirty());assert_eq!(std::fs::read(&source).unwrap(),bytes);
        let rows=s.execute("file.history",&json!({"action":"list"})).unwrap();assert_eq!(rows["versions"][0]["label"],"Before restore");let preview=s.execute("file.history",&json!({"action":"preview","id":rows["versions"][0]["id"]})).unwrap();assert!(preview["text"].as_str().unwrap().contains("Later unsaved speaker notes"));
        s.execute("edit.undo",&json!({})).unwrap();assert_eq!(serde_json::to_value(&*s.active().unwrap().doc).unwrap(),edited["document"]);
    }
    #[test]
    fn persistent_history_smaller_deck_resets_transient_selection_and_undo_recovers_it(){
        let root=Fixture::new();let mut s=root.session();
        let captured=s.execute("file.history",&json!({"action":"capture"})).unwrap();let id=captured["versions"][0]["id"].as_str().unwrap();
        s.execute("slide.new",&json!({"title":"Later slide"})).unwrap();
        s.execute("slide.notes",&json!({"index":1,"text":"n".repeat(1200)})).unwrap();
        let state=s.active_mut().unwrap();assert_eq!(state.selection.slide,1);
        let slide=&state.doc.slides[1];let shape=slide.shapes[0].id;
        state.selection.slides=vec![slide.id];state.selection.shapes=vec![shape];
        state.selection.text=Some(crate::TextSel{shape,anchor:(0,500),caret:(0,900),cell:None,notes:true});
        let selection=state.selection.clone();let edited=state.doc.clone();
        restore(&mut s,id,true,"").unwrap();
        assert_eq!(s.active().unwrap().doc.slides.len(),1);
        assert_eq!(s.active().unwrap().selection,crate::Selection::default());
        s.execute("edit.undo",&json!({})).unwrap();
        assert_eq!(s.active().unwrap().selection,selection);assert_eq!(s.active().unwrap().doc,edited);
    }
    #[test]
    fn persistent_history_valid_empty_deck_restores_safely_and_undo_preserves_source(){
        let root=Fixture::new();let mut s=root.session();let source=root.0.join("source.deckcraft");let bytes=std::fs::read(&source).unwrap();
        s.execute("slide.delete",&json!({"index":0})).unwrap();assert!(s.active().unwrap().doc.slides.is_empty());
        let capture=s.execute("file.history",&json!({"action":"capture","label":"Empty presentation"})).unwrap();let id=capture["versions"][0]["id"].as_str().unwrap();
        s.execute("slide.new",&json!({"title":"Working replacement"})).unwrap();let working=s.active().unwrap().doc.clone();
        restore(&mut s,id,true,"").unwrap();assert!(s.active().unwrap().doc.slides.is_empty());assert_eq!(s.active().unwrap().selection,crate::Selection::default());
        s.execute("document.inspect",&json!({})).unwrap();
        s.execute("edit.undo",&json!({})).unwrap();assert_eq!(s.active().unwrap().doc,working);assert_eq!(std::fs::read(source).unwrap(),bytes);
    }
    #[test]
    fn persistent_history_invalid_stale_and_protected_archive_preserve_source_and_journal(){
        let root=Fixture::new();let mut s=root.session();let source=root.0.join("source.deckcraft");let bytes=std::fs::read(&source).unwrap();let journal=s.journal.len();
        let key="Disposable history fixture password";let captured=s.execute("file.history",&json!({"action":"capture","historyPassword":key})).unwrap();let id=captured["versions"][0]["id"].as_str().unwrap();assert_eq!(s.journal.len(),journal);assert_eq!(captured["protected"],true);
        let before=facts(&s);assert!(s.execute("file.history",&json!({"action":"list"})).is_err());assert_eq!(facts(&s),before);assert!(restore(&mut s,"invalid",true,key).is_err());assert_eq!(facts(&s),before);
        let d=s.active().unwrap();let stale=json!({"action":"restore","id":id,"document":d.uid,"revision":d.revision,"historyPassword":key,"allowDiscard":true});s.execute("slide.notes",&json!({"index":0,"text":"Newer"})).unwrap();let edited=facts(&s);assert!(s.execute("file.history",&stale).is_err());assert_eq!(facts(&s),edited);assert_eq!(std::fs::read(source).unwrap(),bytes);
    }
}
