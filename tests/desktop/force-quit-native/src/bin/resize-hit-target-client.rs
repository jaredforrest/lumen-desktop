//! Disposable GTK Wayland client for exact native frame hit testing.
use gtk::{gdk, prelude::*};
use serde_json::json;
use std::cell::Cell;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

struct State {
    window: gtk::ApplicationWindow,
    path: PathBuf,
    ticks: Cell<u64>,
    presses: Cell<u64>,
    releases: Cell<u64>,
    motions: Cell<u64>,
    last_request: Cell<Option<(i32, i32)>>,
}

impl State {
    fn publish(&self) {
        self.ticks.set(self.ticks.get() + 1);
        let request = self.path.with_extension("resize");
        if let Ok(bytes) = fs::read(&request) {
            if let Ok(pair) = serde_json::from_slice::<(i32, i32)>(&bytes) {
                if self.last_request.get() != Some(pair) {
                    self.last_request.set(Some(pair));
                    self.window.resize(pair.0, pair.1);
                }
            }
        }
        let value = json!({"pid":std::process::id(),"ticks":self.ticks.get(),
            "presses":self.presses.get(),"releases":self.releases.get(),"motions":self.motions.get(),
            "width":self.window.allocated_width(),"height":self.window.allocated_height()});
        let pending = self.path.with_extension("tmp");
        fs::write(&pending, value.to_string()).expect("state write");
        fs::rename(pending, &self.path).expect("state publish");
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let app_id = args.next().expect("application id");
    let title = args.next().expect("window title");
    let path = PathBuf::from(args.next().expect("state file"));
    glib::set_prgname(Some(&app_id));
    let app = gtk::Application::new(Some(&app_id), gio::ApplicationFlags::FLAGS_NONE);
    app.connect_activate(move |app| {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title(&title);
        window.set_default_size(300, 200);
        let geometry = gdk::Geometry::new(180, 120, 0, 0, 0, 0, 0, 0, 0.0, 0.0, gdk::Gravity::NorthWest);
        window.set_geometry_hints(None::<&gtk::Widget>, Some(&geometry), gdk::WindowHints::MIN_SIZE);
        let area = gtk::DrawingArea::new();
        area.add_events(gdk::EventMask::BUTTON_PRESS_MASK | gdk::EventMask::BUTTON_RELEASE_MASK | gdk::EventMask::POINTER_MOTION_MASK);
        window.add(&area);
        let state = Rc::new(State { window:window.clone(), path:path.clone(), ticks:Cell::new(0),
            presses:Cell::new(0), releases:Cell::new(0), motions:Cell::new(0), last_request:Cell::new(None) });
        let pressed = state.clone();
        area.connect_button_press_event(move |_, _| { pressed.presses.set(pressed.presses.get()+1); pressed.publish(); glib::Propagation::Stop });
        let released = state.clone();
        area.connect_button_release_event(move |_, _| { released.releases.set(released.releases.get()+1); released.publish(); glib::Propagation::Stop });
        let moved = state.clone();
        area.connect_motion_notify_event(move |_, _| { moved.motions.set(moved.motions.get()+1); moved.publish(); glib::Propagation::Stop });
        window.show_all();
        state.publish();
        glib::timeout_add_local(Duration::from_millis(30), move || { state.publish(); glib::ControlFlow::Continue });
    });
    app.run_with_args(&[] as &[&str]);
}
