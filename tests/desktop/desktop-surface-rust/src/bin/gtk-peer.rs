//! Disposable GTK URI peer. All controls and receipts stay inside the fixture namespace.
use gtk::{gdk, glib, prelude::*};
use serde_json::{Value, json};
use std::{cell::RefCell, fs, path::PathBuf, rc::Rc, time::Duration};

struct Peer {
    state: Value,
    config: Value,
    output: PathBuf,
    failed: bool,
}
fn action(value: u64) -> gdk::DragAction {
    match value {
        1 => gdk::DragAction::COPY,
        2 => gdk::DragAction::MOVE,
        4 => gdk::DragAction::LINK,
        _ => gdk::DragAction::empty(),
    }
}
impl Peer {
    fn save(&self) {
        let temporary = self.output.with_extension("pending");
        let result = fs::write(
            &temporary,
            serde_json::to_vec(&self.state).expect("Serializable peer receipt"),
        )
        .and_then(|()| fs::rename(temporary, &self.output));
        if let Err(error) = result {
            eprintln!("GTK peer receipt failed: {error}");
            std::process::exit(1);
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let control = PathBuf::from(args.next().ok_or("Missing private control path")?);
    let output = PathBuf::from(args.next().ok_or("Missing private receipt path")?);
    if args.next().is_some()
        || !control.starts_with("/artifacts")
        || !output.starts_with("/artifacts")
    {
        return Err("Private artifact paths required".into());
    }
    gtk::init()?;
    let peer = Rc::new(RefCell::new(Peer {
        state: json!({"ready":true,"received":[],"rawReceived":"","consumed":[],"consumerError":"","dragActive":false,"receiveAction":0,"finished":0,"action":0,"offerActions":1,"deleteRequests":0,"configGeneration":0}),
        config: json!({}),
        output,
        failed: false,
    }));
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("FilesDropPeer");
    window.set_default_size(650, 900);
    window.set_decorated(false);
    let fixed = gtk::Fixed::new();
    window.add(&fixed);
    let source = gtk::EventBox::new();
    source.set_size_request(220, 100);
    source.add(&gtk::Label::new(Some("Disposable GTK file offer")));
    fixed.put(&source, 60, 60);
    let targets = [gtk::TargetEntry::new(
        "text/uri-list",
        gtk::TargetFlags::OTHER_APP,
        0,
    )];
    source.drag_source_set(
        gdk::ModifierType::BUTTON1_MASK,
        &targets,
        gdk::DragAction::COPY,
    );
    fixed.drag_dest_set(gtk::DestDefaults::ALL, &targets, gdk::DragAction::COPY);
    let state = peer.clone();
    fixed.connect_drag_data_received(move |_, context, _, _, data, _, time| {
        let urls = data
            .uris()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let accepted = !urls.is_empty() && context.actions().contains(gdk::DragAction::COPY);
        if accepted {
            let mut state = state.borrow_mut();
            let mut consumed = Vec::new();
            for uri in &urls {
                let result = (|| -> Result<Value, Box<dyn std::error::Error>> {
                    if !uri.starts_with("file:///") {
                        return Err("Nonlocal offered URI".into());
                    }
                    let file = gtk::gio::File::for_uri(uri);
                    let path = gtk::gio::prelude::FileExt::path(&file)
                        .ok_or("Offered URI has no local file")?;
                    let mut bytes = Vec::new();
                    use std::io::Read;
                    fs::File::open(path)?
                        .take(1_048_577)
                        .read_to_end(&mut bytes)?;
                    if bytes.len() > 1_048_576 {
                        return Err("Offered fixture file exceeds budget".into());
                    }
                    Ok(json!({"uri":uri,"text":String::from_utf8(bytes)?}))
                })();
                match result {
                    Ok(value) => consumed.push(value),
                    Err(error) => state.state["consumerError"] = json!(error.to_string()),
                }
            }
            state.state["consumed"] = json!(consumed);
            state.state["received"] = json!(urls);
            state.state["rawReceived"] = json!(String::from_utf8_lossy(&data.data()).to_string());
            state.state["receiveAction"] = json!(1);
            state.save();
        }
        context.drag_finish(accepted, false, time);
    });
    let state = peer.clone();
    source.connect_drag_data_get(move |_, _, selection, _, _| {
        let state = state.borrow();
        let uri = state.config["uri"].as_str().unwrap_or("");
        let success = if state.config["mime"].as_str().unwrap_or("text/uri-list") == "text/uri-list"
        {
            selection.set_uris(&[uri])
        } else {
            selection.set_text(uri)
        };
        if !success {
            eprintln!("GTK peer URI provider rejected payload");
        }
    });
    let state = peer.clone();
    source.connect_drag_begin(move |_, _| {
        let mut state = state.borrow_mut();
        state.failed = false;
        state.state["dragActive"] = json!(true);
        state.save();
    });
    let state = peer.clone();
    source.connect_drag_failed(move |_, _, _| {
        state.borrow_mut().failed = true;
        glib::Propagation::Proceed
    });
    let state = peer.clone();
    source.connect_drag_end(move |_, context| {
        let mut state = state.borrow_mut();
        let selected = context.selected_action();
        let action = if state.failed {
            0
        } else if selected == gdk::DragAction::COPY {
            1
        } else if selected == gdk::DragAction::MOVE {
            2
        } else if selected == gdk::DragAction::LINK {
            4
        } else {
            0
        };
        let finished = state.state["finished"].as_u64().unwrap_or(0) + 1;
        state.state["finished"] = json!(finished);
        state.state["dragActive"] = json!(false);
        state.state["action"] = json!(action);
        state.state["rawGtkSelectedAction"] = json!(selected.bits());
        state.save();
    });
    let state = peer.clone();
    source.connect_drag_data_delete(move |_, _| {
        let mut state = state.borrow_mut();
        let count = state.state["deleteRequests"].as_u64().unwrap_or(0) + 1;
        state.state["deleteRequests"] = json!(count);
        state.save();
    });
    let state = peer.clone();
    glib::timeout_add_local(Duration::from_millis(20), move || {
        let next = fs::read(&control)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if let Some(next) = next {
            let mut state = state.borrow_mut();
            if state.config != next {
                if next["clear"] != state.config["clear"] {
                    state.state["received"] = json!([]);
                    state.state["rawReceived"] = json!("");
                    state.state["consumed"] = json!([]);
                    state.state["consumerError"] = json!("");
                    state.state["receiveAction"] = json!(0);
                }
                let semantic = next["actions"].as_u64().unwrap_or(1);
                let mime = next["mime"].as_str().unwrap_or("text/uri-list");
                let offered = [gtk::TargetEntry::new(mime, gtk::TargetFlags::OTHER_APP, 0)];
                source.drag_source_set(gdk::ModifierType::BUTTON1_MASK, &offered, action(semantic));
                state.state["offerActions"] = json!(semantic);
                state.state["rawGtkOfferedActions"] = json!(action(semantic).bits());
                state.state["configGeneration"] = next["generation"].clone();
                state.config = next;
                state.save();
            }
        }
        glib::ControlFlow::Continue
    });
    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    peer.borrow().save();
    gtk::main();
    Ok(())
}
