//! Disposable dirty GTK document with a real app-owned save prompt.
use gio::prelude::*;
use gtk::prelude::*;
use serde_json::json;
use std::cell::RefCell;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

const APP_ID: &str = "org.example.SessionDocument";
const DOCUMENT: &str = "Private unsaved fixture text";

struct State {
    root: PathBuf,
    window: Option<gtk::ApplicationWindow>,
    dialog: Option<gtk::Dialog>,
    requests: u32,
    choice: String,
}
impl State {
    fn status(&self) {
        fs::write(
            self.root.join("client-state.json"),
            json!({"requests":self.requests,"prompt":self.dialog.is_some(),"choice":self.choice})
                .to_string(),
        )
        .expect("client status");
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env::args().nth(1).ok_or("Missing fixture directory")?);
    glib::set_prgname(Some(APP_ID));
    let app = gtk::Application::new(Some(APP_ID), gio::ApplicationFlags::FLAGS_NONE);
    let state = Rc::new(RefCell::new(State {
        root,
        window: None,
        dialog: None,
        requests: 0,
        choice: String::new(),
    }));
    let started = state.clone();
    app.connect_startup(move |app| {
        let action = gio::SimpleAction::new("quit", None);
        let quit_state = started.clone();
        let quit_app = app.clone();
        action.connect_activate(move |_, _| {
            let mut state = quit_state.borrow_mut();
            state.requests += 1;
            if state.dialog.is_none() {
                let dialog = gtk::Dialog::new();
                dialog.set_title("Save session document?");
                dialog.set_transient_for(state.window.as_ref());
                dialog.set_modal(true);
                dialog.add_button("_Cancel", gtk::ResponseType::Cancel);
                dialog.add_button("_Discard", gtk::ResponseType::Other(2));
                dialog.add_button("_Save", gtk::ResponseType::Other(1));
                dialog.set_default_response(gtk::ResponseType::Cancel);
                dialog.connect_key_press_event(|dialog, event| {
                    let key = *event.keyval();
                    let response = if key == 0xff1b {
                        Some(gtk::ResponseType::Cancel)
                    } else if event.state().contains(gdk::ModifierType::MOD1_MASK) {
                        match key {
                            0x73 => Some(gtk::ResponseType::Other(1)),
                            0x64 => Some(gtk::ResponseType::Other(2)),
                            0x63 => Some(gtk::ResponseType::Cancel),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    if let Some(response) = response {
                        dialog.response(response);
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    }
                });
                let response_state = quit_state.clone();
                let response_app = quit_app.clone();
                dialog.connect_response(move |dialog, response| {
                    let mut state = response_state.borrow_mut();
                    if response == gtk::ResponseType::Other(1) {
                        fs::write(state.root.join("saved.txt"), DOCUMENT).expect("saved document");
                    }
                    state.dialog = None;
                    dialog.hide();
                    state.choice = match response {
                        gtk::ResponseType::Other(1) => "save",
                        gtk::ResponseType::Other(2) => "discard",
                        _ => "cancel",
                    }
                    .into();
                    state.status();
                    if response == gtk::ResponseType::Other(1)
                        || response == gtk::ResponseType::Other(2)
                    {
                        response_app.quit();
                    }
                });
                dialog.show_all();
                state.dialog = Some(dialog);
            }
            state.status();
        });
        app.add_action(&action);
    });
    let activated = state.clone();
    app.connect_activate(move |app| {
        let mut state = activated.borrow_mut();
        let window = gtk::ApplicationWindow::new(app);
        window.set_title("Unsaved session document");
        window.set_default_size(400, 240);
        window.add(&gtk::Label::new(Some(DOCUMENT)));
        window.show_all();
        state.window = Some(window);
        state.status();
    });
    app.run_with_args(&[] as &[&str]);
    Ok(())
}
