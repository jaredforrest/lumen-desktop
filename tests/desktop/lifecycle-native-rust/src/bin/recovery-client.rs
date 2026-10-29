//! An actual dirty GTK text document in the private native recovery session.
use gtk::prelude::*;
use serde_json::json;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let root = PathBuf::from(args.next().ok_or("Missing fixture directory")?);
    let name = args.next().ok_or("Missing document name")?;
    if name != "A" && name != "B" { return Err("Document name must be A or B".into()); }
    let app_id = format!("org.lumen.Recovery{name}");
    glib::set_prgname(Some(&app_id));
    gtk::init()?;
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title(&format!("Unsaved recovery document {name}"));
    window.set_default_size(420, 250);
    let view = gtk::TextView::new();
    let buffer = view.buffer().ok_or("Missing GTK text buffer")?;
    window.add(&view);
    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    view.grab_focus();
    let status = root.join(format!("client-{name}.json"));
    glib::timeout_add_local(Duration::from_millis(30), move || {
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true)
            .map(|value| value.to_string()).unwrap_or_default();
        fs::write(&status, json!({"pid":std::process::id(),"name":name,
            "modified":buffer.is_modified(),"text":text,"focused":view.has_focus()}).to_string())
            .expect("Recovery document status");
        glib::ControlFlow::Continue
    });
    gtk::main();
    Ok(())
}
