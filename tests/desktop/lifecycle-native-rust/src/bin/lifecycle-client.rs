//! Disposable two-document GTK application with an advertised cooperative Quit action.
use gio::prelude::*;
use gtk::prelude::*;
use serde_json::json;
use std::cell::RefCell;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const APP_ID: &str = "org.example.Lifecycle";
const DOCUMENTS: [&str; 2] = ["dirty first", "dirty second"];

struct State {
    root: PathBuf,
    windows: Vec<gtk::ApplicationWindow>,
    requests: u32,
    last_command: String,
    duplicate_titles: bool,
    generation: u32,
}
impl State {
    fn status(&self) {
        fs::write(
            self.root.join("client-state.json"),
            json!({"windows":self.windows.len(),"requests":self.requests,"documents":DOCUMENTS,"generation":self.generation})
                .to_string(),
        )
        .expect("client status");
    }
}
fn add_window(app: &gtk::Application, state: &mut State, index: usize) {
    let window = gtk::ApplicationWindow::new(app);
    window.set_title(&if state.duplicate_titles {
        "Duplicate document".into()
    } else {
        format!("Lifecycle document {}", index + 1)
    });
    window.set_default_size(300, 200);
    window.add(&gtk::Label::new(Some(DOCUMENTS[index % 2])));
    window.show_all();
    state.windows.push(window);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env::args().nth(1).ok_or("Missing fixture directory")?);
    let identity = env::args().nth(2).unwrap_or_else(|| APP_ID.into());
    if !gio::Application::id_is_valid(&identity) {
        return Err("Invalid fixture application ID".into());
    }
    glib::set_prgname(Some(&identity));
    let app = gtk::Application::new(Some(&identity), gio::ApplicationFlags::FLAGS_NONE);
    let state = Rc::new(RefCell::new(State {
        root,
        windows: Vec::new(),
        requests: 0,
        last_command: String::new(),
        duplicate_titles: env::args().nth(3).as_deref() == Some("--duplicate-titles"),
        generation: 0,
    }));
    let started = state.clone();
    app.connect_startup(move |app| {
        std::mem::forget(app.hold());
        let action = gio::SimpleAction::new("quit", None);
        let quit_state = started.clone();
        let quit_app = app.clone();
        action.connect_activate(move |_, _| {
            let mut state = quit_state.borrow_mut();
            state.requests += 1;
            let choice =
                fs::read_to_string(state.root.join("choice")).unwrap_or_else(|_| "cancel".into());
            if choice.trim() == "save" {
                for (index, document) in DOCUMENTS.iter().enumerate() {
                    fs::write(state.root.join(format!("saved-{index}.txt")), document)
                        .expect("saved document");
                }
            }
            state.status();
            if choice.trim() == "save" || choice.trim() == "discard" {
                quit_app.quit();
            }
        });
        app.add_action(&action);
        let polling = started.clone();
        let polling_app = app.clone();
        glib::timeout_add_local(Duration::from_millis(30), move || {
            let mut state = polling.borrow_mut();
            if let Ok(command) = fs::read_to_string(state.root.join("command")) {
                if command != state.last_command {
                    state.last_command = command.clone();
                    if command.starts_with("hide") {
                        for window in &state.windows {
                            window.close();
                        }
                        state.windows.clear();
                        state.status();
                    } else if command.trim() == "close second" || command.trim() == "replace second"
                    {
                        if state.windows.len() > 1 {
                            state.windows.remove(1).close();
                        }
                        if command.trim() == "replace second" {
                            add_window(&polling_app, &mut state, 1);
                        }
                        state.generation += 1;
                        state.status();
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    });
    let activated = state.clone();
    app.connect_activate(move |app| {
        let mut state = activated.borrow_mut();
        if state.windows.is_empty() {
            for index in 0..DOCUMENTS.len() {
                add_window(app, &mut state, index);
            }
        }
        state.status();
    });
    app.run_with_args(&[] as &[&str]);
    Ok(())
}
