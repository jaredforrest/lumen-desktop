//! Same-process X11 window and DBusMenu exporter for the native menu fixture.
use glib::translate::ToGlibPtr;
use gtk::prelude::*;
use serde_json::json;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Node = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

#[link(name = "gdk-3")]
unsafe extern "C" {
    fn gdk_x11_window_get_xid(window: *mut std::ffi::c_void) -> u32;
}

#[derive(Default)]
struct State {
    revision: u32,
    disabled: bool,
    xid: u32,
    keys: Vec<u32>,
}

fn value<T: Into<Value<'static>>>(input: T) -> OwnedValue {
    OwnedValue::try_from(input.into()).expect("menu variant")
}

fn node(id: i32, label: &str, children: Option<Vec<OwnedValue>>, disabled: bool) -> Node {
    let mut properties = HashMap::from([
        ("label".to_owned(), value(label.to_owned())),
        ("enabled".to_owned(), OwnedValue::from(!disabled)),
        ("visible".to_owned(), OwnedValue::from(true)),
    ]);
    if children.is_some() {
        properties.insert("children-display".to_owned(), value("submenu".to_owned()));
    }
    (id, properties, children.unwrap_or_default())
}

fn variant_node(node: Node) -> OwnedValue {
    value(node)
}

fn layout(disabled: bool) -> Node {
    let shortcut = value(vec![vec!["Control".to_owned(), "K".to_owned()]]);
    let unavailable = {
        let mut item = node(11, "Unavailable", None, true);
        item.1.insert("enabled".to_owned(), OwnedValue::from(false));
        variant_node(item)
    };
    let separator = {
        let mut item = node(12, "", None, false);
        item.1
            .insert("type".to_owned(), value("separator".to_owned()));
        variant_node(item)
    };
    let checked = {
        let mut item = node(13, "Checked Command", None, disabled);
        item.1
            .insert("toggle-type".to_owned(), value("checkmark".to_owned()));
        item.1
            .insert("toggle-state".to_owned(), OwnedValue::from(1_i32));
        item.1.insert(
            "shortcut".to_owned(),
            shortcut.try_clone().expect("shortcut clone"),
        );
        variant_node(item)
    };
    let nested = variant_node(node(
        14,
        "Nested",
        Some(vec![{
            let mut deep = node(141, "Deep Command", None, false);
            deep.1.insert("shortcut".to_owned(), shortcut);
            variant_node(deep)
        }]),
        false,
    ));
    let mut headings = vec![
        variant_node(node(
            10,
            "_File",
            Some(vec![unavailable, separator, checked, nested]),
            false,
        )),
        variant_node(node(15, "Disabled Heading", Some(vec![]), true)),
        variant_node(node(
            20,
            "_Edit",
            Some(vec![variant_node(node(21, "Edit Command", None, false))]),
            false,
        )),
    ];
    for index in 0..8 {
        headings.push(variant_node(node(
            30 + index,
            &format!("Additional Heading {}", index + 1),
            Some(vec![variant_node(node(
                300 + index,
                &format!("Additional Command {}", index + 1),
                None,
                false,
            ))]),
            false,
        )));
    }
    node(0, "", Some(headings), false)
}

#[derive(Clone)]
struct Menu {
    state: Arc<Mutex<State>>,
    output: PathBuf,
    window_commands: mpsc::Sender<bool>,
}

#[interface(name = "com.canonical.dbusmenu")]
impl Menu {
    fn get_layout(&self, _parent: i32, _depth: i32, _properties: Vec<String>) -> (u32, Node) {
        let state = self.state.lock().unwrap();
        (state.revision, layout(state.disabled))
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn event(&self, id: i32, event: &str, _data: OwnedValue, _timestamp: u32) {
        let revision = self.state.lock().unwrap().revision;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.output.join("events.jsonl"))
            .expect("events file");
        writeln!(
            file,
            "{}",
            json!({"id":id,"revision":revision,"event":event,"pid":std::process::id()})
        )
        .expect("event record");
    }

    #[zbus(signal)]
    async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;
}

#[derive(Clone)]
struct Control(Menu);

#[interface(name = "org.lumen.TestMenuExporter")]
impl Control {
    async fn change(
        &self,
        kind: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let revision = {
            let mut state = self.0.state.lock().unwrap();
            state.revision = state.revision.wrapping_add(1);
            if kind == "disable" {
                state.disabled = true;
            }
            state.revision
        };
        Menu::layout_updated(&emitter, revision, 0)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }

    fn hide_window(&self) {
        self.0.window_commands.send(false).expect("hide window");
    }

    fn show_window(&self) {
        self.0.window_commands.send(true).expect("show window");
    }

    fn status(&self) -> String {
        let state = self.0.state.lock().unwrap();
        json!({"revision":state.revision,"pid":std::process::id(),"keys":state.keys,"xid":state.xid}).to_string()
    }
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    if args.next().as_deref() != Some("--output") {
        return Err("usage: lumen-menu-exporter-fixture --output DIR".into());
    }
    let output = PathBuf::from(args.next().ok_or("missing output directory")?);
    gtk::init()?;
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Menu Exporter Fixture");
    window.set_default_size(540, 360);
    window.add(&gtk::Label::new(Some("Private exported menu input")));
    let state = Arc::new(Mutex::new(State {
        revision: 1,
        ..State::default()
    }));
    let key_state = state.clone();
    window.connect_key_press_event(move |_, event| {
        key_state.lock().unwrap().keys.push(*event.keyval());
        glib::Propagation::Proceed
    });
    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    let gdk_window = window.window().ok_or("window not realized")?;
    let pointer: *mut gdk::ffi::GdkWindow = gdk_window.to_glib_none().0;
    let xid = unsafe { gdk_x11_window_get_xid(pointer.cast()) };
    state.lock().unwrap().xid = xid;
    let (sender, receiver) = mpsc::channel();
    let controlled_window = window.clone();
    glib::timeout_add_local(Duration::from_millis(20), move || {
        while let Ok(show) = receiver.try_recv() {
            if show {
                controlled_window.show_all();
            } else {
                controlled_window.hide();
            }
        }
        glib::ControlFlow::Continue
    });
    let menu = Menu {
        state,
        output: output.clone(),
        window_commands: sender,
    };
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let result: Result<()> = async {
                let connection = zbus::connection::Builder::session()?
                    .name("org.lumen.TestMenuExporter")?
                    .serve_at("/Menu", menu.clone())?
                    .serve_at("/Menu", Control(menu))?
                    .build()
                    .await?;
                let registrar = zbus::Proxy::new(
                    &connection,
                    "com.canonical.AppMenu.Registrar",
                    "/com/canonical/AppMenu/Registrar",
                    "com.canonical.AppMenu.Registrar",
                )
                .await?;
                registrar
                    .call::<_, _, ()>("RegisterWindow", &(xid, ObjectPath::try_from("/Menu")?))
                    .await?;
                ready_tx.send(Ok(())).ok();
                std::future::pending::<()>().await;
                Ok(())
            }
            .await;
            if let Err(error) = result {
                let _ = ready_tx.send(Err(error.to_string()));
            }
        });
    });
    ready_rx
        .recv()?
        .map_err(|error| format!("D-Bus registration: {error}"))?;
    fs::write(
        output.join("exporter.json"),
        json!({"pid":std::process::id(),"xid":xid}).to_string(),
    )?;
    gtk::main();
    Ok(())
}
