//! Synthetic GTK dialog test. No authentication or password handling occurs.
use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

#[link(name = "gtk-3")]
unsafe extern "C" {
    fn gtk_init(argc: *mut c_int, argv: *mut *mut *mut c_char);
    fn gtk_dialog_new() -> *mut c_void;
    fn gtk_window_set_title(window: *mut c_void, title: *const c_char);
    fn gtk_dialog_get_content_area(dialog: *mut c_void) -> *mut c_void;
    fn gtk_label_new(text: *const c_char) -> *mut c_void;
    fn gtk_container_add(container: *mut c_void, widget: *mut c_void);
    fn gtk_widget_show_all(widget: *mut c_void);
    fn gtk_events_pending() -> c_int;
    fn gtk_main_iteration() -> c_int;
    fn gtk_settings_get_default() -> *mut c_void;
    fn gtk_widget_get_visible(widget: *mut c_void) -> c_int;
    fn gtk_widget_destroy(widget: *mut c_void);
}
#[link(name = "gio-2.0")]
unsafe extern "C" {
    fn g_settings_new(schema: *const c_char) -> *mut c_void;
    fn g_settings_set_string(
        settings: *mut c_void,
        key: *const c_char,
        value: *const c_char,
    ) -> c_int;
}
#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_object_get(object: *mut c_void, first_property: *const c_char, ...);
    fn g_object_unref(object: *mut c_void);
}
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_free(value: *mut c_void);
}

fn main() {
    unsafe {
        let mut argc = 0;
        let mut argv = ptr::null_mut();
        gtk_init(&mut argc, &mut argv);
        let preferences = g_settings_new(c"org.gnome.desktop.interface".as_ptr());
        assert!(!preferences.is_null(), "Appearance schema unavailable");
        let dialog = gtk_dialog_new();
        assert!(!dialog.is_null(), "Synthetic dialog unavailable");
        gtk_window_set_title(dialog, c"Authentication required".as_ptr());
        gtk_container_add(
            gtk_dialog_get_content_area(dialog),
            gtk_label_new(c"Protected setting".as_ptr()),
        );
        gtk_widget_show_all(dialog);
        let themes = [
            c"Adwaita",
            c"Adwaita-dark",
            c"Adwaita",
            c"Adwaita",
        ];
        let schemes = [
            c"default",
            c"prefer-dark",
            c"prefer-light",
            c"default",
        ];
        for index in 0..4 {
            assert_ne!(
                g_settings_set_string(preferences, c"gtk-theme".as_ptr(), themes[index].as_ptr()),
                0,
                "Theme update refused"
            );
            assert_ne!(
                g_settings_set_string(
                    preferences,
                    c"color-scheme".as_ptr(),
                    schemes[index].as_ptr(),
                ),
                0,
                "Color scheme update refused"
            );
            while gtk_events_pending() != 0 {
                gtk_main_iteration();
            }
            let mut actual_theme = ptr::null_mut::<c_char>();
            let mut actual_dark: c_int = 0;
            g_object_get(
                gtk_settings_get_default(),
                c"gtk-theme-name".as_ptr(),
                &mut actual_theme,
                c"gtk-application-prefer-dark-theme".as_ptr(),
                &mut actual_dark,
                ptr::null::<c_char>(),
            );
            assert!(!actual_theme.is_null(), "GTK theme property absent");
            assert_eq!(
                CStr::from_ptr(actual_theme).to_bytes(),
                themes[index].to_bytes(),
                "GTK theme did not follow appearance"
            );
            assert_eq!(
                actual_dark != 0,
                index == 1,
                "GTK dark preference did not follow appearance"
            );
            g_free(actual_theme.cast());
            assert_ne!(gtk_widget_get_visible(dialog), 0, "Dialog vanished");
        }
        gtk_widget_destroy(dialog);
        g_object_unref(preferences);
    }
    println!("PASS: module initialisation and live light/dark/default theme propagation");
}
