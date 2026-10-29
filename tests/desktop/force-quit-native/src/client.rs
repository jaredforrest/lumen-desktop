//! Disposable native GTK client with a dirty, physically editable document.
use gtk::prelude::*;
use serde_json::json;
use std::cell::Cell;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

fn main() {
    let mut args = env::args().skip(1);
    let app_id = args.next().expect("application id");
    let title = args.next().expect("window title");
    let state = PathBuf::from(args.next().expect("state file"));
    glib::set_prgname(Some(&app_id));
    let app = gtk::Application::new(Some(&app_id), gio::ApplicationFlags::FLAGS_NONE);
    let ticks = Rc::new(Cell::new(0u64));
    app.connect_activate(move |app| {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title(&title);
        window.set_default_size(430, 230);
        let editor = gtk::Entry::new();
        editor.set_text("unsaved-synthetic-");
        window.add(&editor);
        window.show_all();
        editor.grab_focus();
        editor.set_position(-1);
        let ticks = ticks.clone();
        let state = state.clone();
        let publish = move || {
            let next = ticks.get() + 1;
            ticks.set(next);
            let value = json!({"pid":std::process::id(),"ticks":next,"text":editor.text().as_str(),"dirty":true});
            let pending = state.with_extension("tmp");
            fs::write(&pending, value.to_string()).expect("state write");
            fs::rename(pending, &state).expect("state publish");
            glib::ControlFlow::Continue
        };
        publish();
        glib::timeout_add_local(Duration::from_millis(40), publish);
    });
    app.run_with_args(&[] as &[&str]);
}
