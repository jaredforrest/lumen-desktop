//! GTK module loaded by the authentication-agent wrapper.
//! The exported symbol is GTK's C ABI; the implementation is Rust 2024.
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

static APPEARANCE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

#[link(name = "gio-2.0")]
unsafe extern "C" {
    fn g_settings_schema_source_new_from_directory(
        directory: *const c_char,
        parent: *mut c_void,
        trusted: c_int,
        error: *mut *mut c_void,
    ) -> *mut c_void;
    fn g_settings_schema_source_get_default() -> *mut c_void;
    fn g_settings_schema_source_lookup(
        source: *mut c_void,
        id: *const c_char,
        recursive: c_int,
    ) -> *mut c_void;
    fn g_settings_schema_source_unref(source: *mut c_void);
    fn g_settings_schema_unref(schema: *mut c_void);
    fn g_settings_schema_has_key(schema: *mut c_void, key: *const c_char) -> c_int;
    fn g_settings_new_full(
        schema: *mut c_void,
        backend: *mut c_void,
        path: *const c_char,
    ) -> *mut c_void;
    fn g_settings_bind(
        settings: *mut c_void,
        key: *const c_char,
        object: *mut c_void,
        property: *const c_char,
        flags: c_int,
    );
    fn g_settings_bind_with_mapping(
        settings: *mut c_void,
        key: *const c_char,
        object: *mut c_void,
        property: *const c_char,
        flags: c_int,
        get_mapping: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> c_int>,
        set_mapping: *mut c_void,
        user_data: *mut c_void,
        destroy: *mut c_void,
    );
}
#[link(name = "gtk-3")]
unsafe extern "C" {
    fn gtk_settings_get_default() -> *mut c_void;
}
#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_value_set_boolean(value: *mut c_void, boolean: c_int);
}
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_variant_get_string(variant: *mut c_void, length: *mut usize) -> *const c_char;
}

unsafe extern "C" fn map_color_scheme(
    value: *mut c_void,
    variant: *mut c_void,
    _user_data: *mut c_void,
) -> c_int {
    if value.is_null() || variant.is_null() {
        return 0;
    }
    let raw = unsafe { g_variant_get_string(variant, ptr::null_mut()) };
    if raw.is_null() {
        return 0;
    }
    let dark = unsafe { CStr::from_ptr(raw) }.to_bytes() == b"prefer-dark";
    unsafe { g_value_set_boolean(value, i32::from(dark)) };
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gtk_module_init(_argc: *mut c_int, _argv: *mut *mut *mut c_char) {
    if !APPEARANCE.load(Ordering::Acquire).is_null() {
        return;
    }
    let Ok(directory) = CString::new(env!("DESKTOP_SCHEMA_DIR")) else {
        return;
    };
    let source = unsafe {
        g_settings_schema_source_new_from_directory(
            directory.as_ptr(),
            g_settings_schema_source_get_default(),
            0,
            ptr::null_mut(),
        )
    };
    if source.is_null() {
        return;
    }
    let schema = unsafe {
        g_settings_schema_source_lookup(
            source,
            c"org.gnome.desktop.interface".as_ptr(),
            1,
        )
    };
    unsafe { g_settings_schema_source_unref(source) };
    if schema.is_null() {
        return;
    }
    let settings = unsafe { gtk_settings_get_default() };
    if settings.is_null() {
        unsafe { g_settings_schema_unref(schema) };
        return;
    }
    let appearance = unsafe { g_settings_new_full(schema, ptr::null_mut(), ptr::null()) };
    if appearance.is_null() {
        unsafe { g_settings_schema_unref(schema) };
        return;
    }
    APPEARANCE.store(appearance, Ordering::Release);
    unsafe {
        g_settings_bind(
            appearance,
            c"gtk-theme".as_ptr(),
            settings,
            c"gtk-theme-name".as_ptr(),
            1,
        );
        if g_settings_schema_has_key(schema, c"color-scheme".as_ptr()) != 0 {
            g_settings_bind_with_mapping(
                appearance,
                c"color-scheme".as_ptr(),
                settings,
                c"gtk-application-prefer-dark-theme".as_ptr(),
                1,
                Some(map_color_scheme),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
        }
        g_settings_schema_unref(schema);
    }
}
