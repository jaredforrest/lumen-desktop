//! Actual GTK framebuffer and editable document in the private remote session.
use serde_json::{Value, json};
use std::{
    ffi::{CStr, c_char, c_int, c_void},
    sync::{Arc, Mutex},
};
#[link(name = "gtk-3")]
unsafe extern "C" {
    fn gtk_init(argc: *mut c_int, argv: *mut *mut *mut c_char);
    fn gtk_main();
    fn gtk_main_quit();
    fn gtk_window_new(kind: c_int) -> *mut c_void;
    fn gtk_window_set_title(window: *mut c_void, title: *const c_char);
    fn gtk_window_set_default_size(window: *mut c_void, width: c_int, height: c_int);
    fn gtk_window_set_decorated(window: *mut c_void, decorated: c_int);
    fn gtk_box_new(orientation: c_int, spacing: c_int) -> *mut c_void;
    fn gtk_container_add(container: *mut c_void, child: *mut c_void);
    fn gtk_box_pack_start(
        container: *mut c_void,
        child: *mut c_void,
        expand: c_int,
        fill: c_int,
        padding: u32,
    );
    fn gtk_grid_new() -> *mut c_void;
    fn gtk_grid_set_column_homogeneous(grid: *mut c_void, homogeneous: c_int);
    fn gtk_grid_set_row_homogeneous(grid: *mut c_void, homogeneous: c_int);
    fn gtk_grid_attach(
        grid: *mut c_void,
        child: *mut c_void,
        left: c_int,
        top: c_int,
        width: c_int,
        height: c_int,
    );
    fn gtk_drawing_area_new() -> *mut c_void;
    fn gtk_entry_new() -> *mut c_void;
    fn gtk_text_view_new() -> *mut c_void;
    fn gtk_text_view_get_buffer(view: *mut c_void) -> *mut c_void;
    fn gtk_text_buffer_get_bounds(buffer: *mut c_void, start: *mut c_void, end: *mut c_void);
    fn gtk_text_buffer_get_text(
        buffer: *mut c_void,
        start: *const c_void,
        end: *const c_void,
        hidden: c_int,
    ) -> *mut c_char;
    fn gtk_text_buffer_set_text(buffer: *mut c_void, text: *const c_char, length: c_int);
    fn gtk_entry_get_text(entry: *mut c_void) -> *const c_char;
    fn gtk_entry_set_placeholder_text(entry: *mut c_void, text: *const c_char);
    fn gtk_widget_show_all(widget: *mut c_void);
    fn gtk_widget_grab_focus(widget: *mut c_void);
}
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_set_prgname(name: *const c_char);
    fn g_free(pointer: *mut c_void);
}
#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_signal_connect_data(
        instance: *mut c_void,
        signal: *const c_char,
        callback: *const c_void,
        data: *mut c_void,
        destroy: *const c_void,
        flags: u32,
    ) -> u64;
}
#[link(name = "cairo")]
unsafe extern "C" {
    fn cairo_set_source_rgb(cr: *mut c_void, r: f64, g: f64, b: f64);
    fn cairo_paint(cr: *mut c_void);
}
const COLORS: [[u8; 3]; 4] = [
    [0xd0, 0x40, 0x40],
    [0x40, 0xc0, 0x80],
    [0x40, 0x70, 0xd0],
    [0xe0, 0xb0, 0x40],
];
#[derive(Default)]
struct State {
    text: String,
    keys: Vec<Value>,
}
struct Target(Arc<Mutex<State>>);
#[zbus::interface(name = "org.lumen.RemoteTarget")]
impl Target {
    fn status(&self) -> String {
        let state = self.0.lock().unwrap();
        json!({"pid":std::process::id(),"text":state.text,"keys":state.keys,"colors":["#d04040","#40c080","#4070d0","#e0b040"]}).to_string()
    }
}
unsafe extern "C" fn draw(_: *mut c_void, cr: *mut c_void, index: *mut c_void) -> c_int {
    let color = COLORS[index as usize];
    unsafe {
        cairo_set_source_rgb(
            cr,
            color[0] as f64 / 255.,
            color[1] as f64 / 255.,
            color[2] as f64 / 255.,
        );
        cairo_paint(cr);
    }
    0
}
unsafe extern "C" fn changed(entry: *mut c_void, state: *mut c_void) {
    unsafe {
        let shared = &*(state as *const Arc<Mutex<State>>);
        shared.lock().unwrap().text = CStr::from_ptr(gtk_entry_get_text(entry))
            .to_string_lossy()
            .into_owned();
    }
}
unsafe extern "C" fn document_changed(buffer: *mut c_void, state: *mut c_void) {
    unsafe {
        // Opaque, pointer-aligned storage exceeds GtkTextIter on supported 64-bit targets.
        let mut start = [0usize; 16];
        let mut end = [0usize; 16];
        gtk_text_buffer_get_bounds(buffer, start.as_mut_ptr().cast(), end.as_mut_ptr().cast());
        let text = gtk_text_buffer_get_text(buffer, start.as_ptr().cast(), end.as_ptr().cast(), 1);
        let shared = &*(state as *const Arc<Mutex<State>>);
        shared.lock().unwrap().text = CStr::from_ptr(text).to_string_lossy().into_owned();
        g_free(text.cast());
    }
}
#[repr(C)]
struct KeyEvent {
    kind: c_int,
    window: *mut c_void,
    send_event: i8,
    time: u32,
    state: u32,
    keyval: u32,
}
unsafe extern "C" fn key(_: *mut c_void, event: *const KeyEvent, state: *mut c_void) -> c_int {
    unsafe {
        let shared = &*(state as *const Arc<Mutex<State>>);
        shared
            .lock()
            .unwrap()
            .keys
            .push(json!({"keyval":(*event).keyval,"state":(*event).state}));
    }
    0
}
unsafe extern "C" fn destroy(_: *mut c_void, _: *mut c_void) {
    unsafe {
        gtk_main_quit();
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let local = arguments.iter().any(|argument| argument == "--local");
    let multiline = arguments.iter().any(|argument| argument == "--multiline");
    let seed = arguments
        .windows(2)
        .find(|pair| pair[0] == "--seed-file")
        .map(|pair| std::fs::read_to_string(&pair[1]))
        .transpose()?;
    let shared = Arc::new(Mutex::new(State::default()));
    let _bus = zbus::blocking::connection::Builder::session()?
        .name(if local {
            "org.lumen.LocalTarget"
        } else {
            "org.lumen.RemoteTarget"
        })?
        .serve_at("/Target", Target(shared.clone()))?
        .build()?;
    unsafe {
        g_set_prgname(if local {
            c"org.lumen.LocalTarget".as_ptr()
        } else {
            c"org.lumen.RemoteTarget".as_ptr()
        });
        gtk_init(std::ptr::null_mut(), std::ptr::null_mut());
        let window = gtk_window_new(0);
        gtk_window_set_title(window, c"Disposable remote target".as_ptr());
        gtk_window_set_default_size(window, 1280, 720);
        gtk_window_set_decorated(window, 0);
        let layout = gtk_box_new(1, 0);
        gtk_container_add(window, layout);
        let grid = gtk_grid_new();
        gtk_grid_set_column_homogeneous(grid, 1);
        gtk_grid_set_row_homogeneous(grid, 1);
        gtk_box_pack_start(layout, grid, 1, 1, 0);
        for index in 0..4 {
            let area = gtk_drawing_area_new();
            g_signal_connect_data(
                area,
                c"draw".as_ptr(),
                draw as *const c_void,
                index as *mut c_void,
                std::ptr::null(),
                0,
            );
            gtk_grid_attach(grid, area, (index % 2) as i32, (index / 2) as i32, 1, 1);
        }
        let entry = if multiline {
            gtk_text_view_new()
        } else {
            gtk_entry_new()
        };
        if !multiline {
            gtk_entry_set_placeholder_text(entry, c"Synthetic remote input".as_ptr());
        }
        gtk_box_pack_start(layout, entry, 0, 0, 0);
        let data = Box::into_raw(Box::new(shared)).cast();
        g_signal_connect_data(
            if multiline {
                gtk_text_view_get_buffer(entry)
            } else {
                entry
            },
            c"changed".as_ptr(),
            if multiline {
                document_changed as *const c_void
            } else {
                changed as *const c_void
            },
            data,
            std::ptr::null(),
            0,
        );
        if let Some(seed) = seed {
            if !multiline {
                return Err("Seeded documents require the multiline fixture".into());
            }
            let seed = std::ffi::CString::new(seed)?;
            gtk_text_buffer_set_text(
                gtk_text_view_get_buffer(entry),
                seed.as_ptr(),
                seed.as_bytes().len().try_into()?,
            );
        }
        g_signal_connect_data(
            entry,
            c"key-press-event".as_ptr(),
            key as *const c_void,
            data,
            std::ptr::null(),
            0,
        );
        g_signal_connect_data(
            window,
            c"destroy".as_ptr(),
            destroy as *const c_void,
            std::ptr::null_mut(),
            std::ptr::null(),
            0,
        );
        gtk_widget_show_all(window);
        gtk_widget_grab_focus(entry);
        gtk_main();
        drop(Box::from_raw(data as *mut Arc<Mutex<State>>));
    }
    Ok(())
}
