//! Narrow native GOA/EDS discovery boundary. It reads no credentials or tokens.
use chrono::{Datelike, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_EVENTS: usize = 10_000;
const MAX_RESPONSE: usize = 8 * 1024 * 1024;

#[repr(C)]
struct GList {
    data: *mut c_void,
    next: *mut GList,
    prev: *mut GList,
}

#[repr(C)]
struct GSList {
    data: *mut c_void,
    next: *mut GSList,
}

unsafe extern "C" {
    fn goa_client_new_sync(cancellable: *mut c_void, error: *mut *mut c_void) -> *mut c_void;
    fn goa_client_get_accounts(client: *mut c_void) -> *mut GList;
    fn goa_object_get_account(object: *mut c_void) -> *mut c_void;
    fn goa_account_get_provider_type(account: *mut c_void) -> *const c_char;
    fn goa_account_get_id(account: *mut c_void) -> *const c_char;
    fn goa_account_get_calendar_disabled(account: *mut c_void) -> c_int;
    fn goa_account_get_attention_needed(account: *mut c_void) -> c_int;

    fn e_source_registry_new_sync(cancellable: *mut c_void, error: *mut *mut c_void)
    -> *mut c_void;
    fn e_source_registry_list_sources(
        registry: *mut c_void,
        extension: *const c_char,
    ) -> *mut GList;
    fn e_source_registry_find_extension(
        registry: *mut c_void,
        source: *mut c_void,
        extension: *const c_char,
    ) -> *mut c_void;
    fn e_source_registry_check_enabled(registry: *mut c_void, source: *mut c_void) -> c_int;
    fn e_source_registry_ref_source(registry: *mut c_void, uid: *const c_char) -> *mut c_void;
    fn e_source_get_extension(source: *mut c_void, extension: *const c_char) -> *mut c_void;
    fn e_source_goa_get_account_id(extension: *mut c_void) -> *const c_char;
    fn e_source_get_uid(source: *mut c_void) -> *const c_char;
    fn e_source_get_display_name(source: *mut c_void) -> *const c_char;
    #[cfg(feature = "color")]
    fn e_source_selectable_get_color(extension: *mut c_void) -> *const c_char;
    fn e_cal_client_connect_sync(
        source: *mut c_void,
        source_type: c_int,
        wait_seconds: u32,
        cancellable: *mut c_void,
        error: *mut *mut c_void,
    ) -> *mut c_void;
    fn e_cal_client_get_object_list_sync(
        client: *mut c_void,
        query: *const c_char,
        components: *mut *mut GSList,
        cancellable: *mut c_void,
        error: *mut *mut c_void,
    ) -> c_int;
    fn e_cal_client_get_objects_for_uid_sync(
        client: *mut c_void,
        uid: *const c_char,
        components: *mut *mut GSList,
        cancellable: *mut c_void,
        error: *mut *mut c_void,
    ) -> c_int;
    fn e_cal_client_get_component_as_string(
        client: *mut c_void,
        component: *mut c_void,
    ) -> *mut c_char;
    fn e_cal_client_get_object_sync(
        client: *mut c_void,
        uid: *const c_char,
        rid: *const c_char,
        component: *mut *mut c_void,
        cancellable: *mut c_void,
        error: *mut *mut c_void,
    ) -> c_int;
    fn e_cal_client_create_object_sync(
        client: *mut c_void,
        component: *mut c_void,
        flags: c_int,
        out_uid: *mut *mut c_char,
        cancellable: *mut c_void,
        error: *mut *mut c_void,
    ) -> c_int;
    fn e_cal_client_modify_object_sync(
        client: *mut c_void,
        component: *mut c_void,
        mode: c_int,
        flags: c_int,
        cancellable: *mut c_void,
        error: *mut *mut c_void,
    ) -> c_int;
    fn e_cal_client_remove_object_sync(
        client: *mut c_void,
        uid: *const c_char,
        rid: *const c_char,
        mode: c_int,
        flags: c_int,
        cancellable: *mut c_void,
        error: *mut *mut c_void,
    ) -> c_int;
    fn i_cal_component_new_from_string(text: *const c_char) -> *mut c_void;
    fn e_cal_component_get_icalcomponent(component: *mut c_void) -> *mut c_void;
    fn e_client_util_free_object_slist(list: *mut GSList);
    fn e_client_is_online(client: *mut c_void) -> c_int;
    fn e_client_is_readonly(client: *mut c_void) -> c_int;
    fn e_client_refresh_sync(
        client: *mut c_void,
        cancellable: *mut c_void,
        error: *mut *mut c_void,
    ) -> c_int;
    fn i_cal_component_as_ical_string(component: *mut c_void) -> *mut c_char;

    fn g_object_unref(object: *mut c_void);
    fn g_list_free_full(list: *mut GList, free_func: Option<unsafe extern "C" fn(*mut c_void)>);
    fn g_error_free(error: *mut c_void);
    fn g_free(memory: *mut c_void);
    fn g_cancellable_new() -> *mut c_void;
    fn g_cancellable_cancel(cancellable: *mut c_void);
}

struct Object(*mut c_void);
impl Drop for Object {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Every Object wraps a full GObject reference from a constructor.
            unsafe { g_object_unref(self.0) };
        }
    }
}

struct ObjectList(*mut GSList);
impl Drop for ObjectList {
    fn drop(&mut self) {
        // SAFETY: EDS returns full references for each object in these lists.
        unsafe { e_client_util_free_object_slist(self.0) };
    }
}

struct TimedCall {
    cancellable: Object,
    done: Sender<()>,
    timer: Option<JoinHandle<()>>,
}
impl TimedCall {
    fn new() -> Self {
        // SAFETY: GCancellable is a GObject with a full returned reference.
        let cancellable = Object(unsafe { g_cancellable_new() });
        let address = cancellable.0 as usize;
        let (done, receiver) = mpsc::channel();
        let timer = thread::spawn(move || {
            if receiver.recv_timeout(Duration::from_secs(12)).is_err() {
                // SAFETY: TimedCall keeps this GCancellable referenced until join.
                unsafe { g_cancellable_cancel(address as *mut c_void) };
            }
        });
        Self {
            cancellable,
            done,
            timer: Some(timer),
        }
    }
    fn ptr(&self) -> *mut c_void {
        self.cancellable.0
    }
}
impl Drop for TimedCall {
    fn drop(&mut self) {
        let _ = self.done.send(());
        if let Some(timer) = self.timer.take() {
            let _ = timer.join();
        }
    }
}

fn fixed_error(error: *mut c_void) -> String {
    if !error.is_null() {
        // SAFETY: A failed GLib call gives this function ownership of GError.
        unsafe { g_error_free(error) };
    }
    "Online Accounts or the calendar service is unavailable. Cached events were kept.".into()
}

unsafe fn string(value: *const c_char) -> String {
    if value.is_null() {
        return String::new();
    }
    // SAFETY: Native getter results are NUL-terminated and remain owned by
    // their live source/account object during this conversion.
    unsafe { CStr::from_ptr(value).to_string_lossy().into_owned() }
}

unsafe fn owned_string(value: *mut c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    // SAFETY: EDS/libical returns a newly allocated NUL-terminated string.
    let copy = unsafe { CStr::from_ptr(value).to_string_lossy().into_owned() };
    unsafe { g_free(value.cast()) };
    Some(copy)
}

/// Return the same account/calendar summary shape as the Python discovery path.
/// This function does not fetch event data or inspect authentication material.
pub fn discover() -> Result<Value, String> {
    // SAFETY: The declarations above mirror the installed GOA/EDS C headers.
    // GObject and GList ownership is released at the end of each scope.
    unsafe {
        let mut error = ptr::null_mut();
        let timed = TimedCall::new();
        let goa = Object(goa_client_new_sync(timed.ptr(), &mut error));
        drop(timed);
        if goa.0.is_null() {
            return Err(fixed_error(error));
        }
        let mut accounts = Vec::new();
        let mut account_ids = BTreeSet::new();
        let objects = goa_client_get_accounts(goa.0);
        let mut cursor = objects;
        while !cursor.is_null() {
            let account = Object(goa_object_get_account((*cursor).data));
            if !account.0.is_null()
                && string(goa_account_get_provider_type(account.0)) == "google"
                && goa_account_get_calendar_disabled(account.0) == 0
            {
                let id = string(goa_account_get_id(account.0));
                if !id.is_empty() && account_ids.insert(id.clone()) {
                    accounts.push(json!({"id":id,"attentionNeeded":goa_account_get_attention_needed(account.0)!=0}));
                }
            }
            cursor = (*cursor).next;
        }
        g_list_free_full(objects, Some(g_object_unref));

        error = ptr::null_mut();
        let timed = TimedCall::new();
        let registry = Object(e_source_registry_new_sync(timed.ptr(), &mut error));
        drop(timed);
        if registry.0.is_null() {
            return Err(fixed_error(error));
        }
        let mut calendars = Vec::new();
        let mut seen_accounts = BTreeSet::new();
        let sources = e_source_registry_list_sources(registry.0, c"Calendar".as_ptr());
        let mut cursor = sources;
        while !cursor.is_null() {
            let source = (*cursor).data;
            let ancestor = Object(e_source_registry_find_extension(
                registry.0,
                source,
                c"GNOME Online Accounts".as_ptr(),
            ));
            if !ancestor.0.is_null() {
                let extension =
                    e_source_get_extension(ancestor.0, c"GNOME Online Accounts".as_ptr());
                if !extension.is_null() {
                    let account_id = string(e_source_goa_get_account_id(extension));
                    if account_ids.contains(&account_id) {
                        seen_accounts.insert(account_id.clone());
                        if e_source_registry_check_enabled(registry.0, source) != 0 {
                            let calendar = json!({
                                "id":string(e_source_get_uid(source)),
                                "name":string(e_source_get_display_name(source)),
                                "accountId":account_id,"accessRole":"unknown"
                            });
                            #[cfg(feature = "color")]
                            let calendar = {
                                let mut calendar = calendar;
                                let selectable =
                                    e_source_get_extension(source, c"Calendar".as_ptr());
                                let color = if selectable.is_null() {
                                    String::new()
                                } else {
                                    bounded_color(&string(e_source_selectable_get_color(
                                        selectable,
                                    )))
                                };
                                calendar["color"] = json!(color);
                                calendar
                            };
                            calendars.push(calendar);
                        }
                    }
                }
            }
            cursor = (*cursor).next;
            if calendars.len() > 250 {
                g_list_free_full(sources, Some(g_object_unref));
                return Err("Too many calendars are connected for this desktop view.".into());
            }
        }
        g_list_free_full(sources, Some(g_object_unref));
        Ok(json!({"accounts":accounts,"calendars":calendars,
            "pendingDiscovery":account_ids.iter().any(|id|!seen_accounts.contains(id))}))
    }
}

pub(crate) fn series_identity(raw: &str) -> Option<(String, String, bool)> {
    // EDS components can contain vendor properties that the narrower Rust
    // iCalendar parser rejects. Only these three identity facts are needed to
    // group a full UID series; EDS itself remains the parser of event content.
    let mut lines: Vec<String> = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if (line.starts_with(' ') || line.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut()?.push_str(&line[1..]);
        } else {
            lines.push(line.to_owned());
        }
    }
    let mut uid = None;
    let mut rid = String::new();
    let mut recurring = false;
    for line in lines {
        let Some((property, value)) = line.split_once(':') else {
            continue;
        };
        match property.split(';').next()?.to_ascii_uppercase().as_str() {
            "UID" => uid = Some(value.to_owned()),
            "RECURRENCE-ID" => {
                rid = value.to_owned();
                recurring = true;
            }
            "RRULE" | "RDATE" => recurring = true,
            _ => {}
        }
    }
    let uid = uid.filter(|value| !value.is_empty() && value.len() <= 1024)?;
    Some((uid, rid, recurring))
}

/// EDS sometimes returns a VCALENDAR containing several instances for one UID.
/// Select the exact component before comparing its revision or sending an edit.
pub(crate) fn select_event_text(raw: &str, uid: &str, rid: &str) -> Result<String, String> {
    let mut block = String::new();
    let mut inside = false;
    let mut nested = 0usize;
    for line in raw.lines() {
        let clean = line.trim_end_matches('\r');
        if clean.eq_ignore_ascii_case("BEGIN:VEVENT") && !inside {
            inside = true;
            block.clear();
        }
        if !inside {
            continue;
        }
        block.push_str(clean);
        block.push('\n');
        if clean.to_ascii_uppercase().starts_with("BEGIN:")
            && !clean.eq_ignore_ascii_case("BEGIN:VEVENT")
        {
            nested += 1;
        } else if clean.to_ascii_uppercase().starts_with("END:")
            && !clean.eq_ignore_ascii_case("END:VEVENT")
        {
            nested = nested.saturating_sub(1);
        } else if clean.eq_ignore_ascii_case("END:VEVENT") && nested == 0 {
            inside = false;
            if series_identity(&block)
                .is_some_and(|(event_uid, event_rid, _)| event_uid == uid && event_rid == rid)
            {
                return Ok(block);
            }
        }
    }
    Err("This event no longer exists. Sync and reopen it.".into())
}

#[cfg(feature = "color")]
fn bounded_color(raw: &str) -> String {
    if (raw.len() == 7 || raw.len() == 9)
        && raw.starts_with('#')
        && raw[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        raw[..7].to_owned()
    } else {
        String::new()
    }
}

fn append_component(
    client: *mut c_void,
    component: *mut c_void,
    entries: &mut Vec<Value>,
    seen: &mut BTreeSet<(String, String)>,
    size: &mut usize,
) -> Result<(), String> {
    // SAFETY: Both pointers are live objects owned by the caller's EDS lists.
    let (raw, ical) = unsafe {
        let raw = owned_string(i_cal_component_as_ical_string(component));
        let ical = owned_string(e_cal_client_get_component_as_string(client, component));
        (raw, ical)
    };
    let (Some(raw), Some(ical)) = (raw, ical) else {
        return Err("The calendar service returned too many or unreadable events.".into());
    };
    let Some((uid, rid, _)) = series_identity(&raw) else {
        return Err("The calendar service returned too many or unreadable events.".into());
    };
    if !seen.insert((uid, rid)) {
        return Ok(());
    }
    *size = size.saturating_add(raw.len()).saturating_add(ical.len());
    if *size > MAX_RESPONSE || entries.len() >= MAX_EVENTS {
        return Err(
            "This calendar exceeds the supported cache size. Previous events were kept.".into(),
        );
    }
    entries.push(json!({"raw":raw,"ical":ical}));
    Ok(())
}

/// Fetch a bounded month from EDS. The caller is responsible for caching and
/// generation checks before publishing the result.
pub fn load(uid: &str, month: NaiveDate, tz: Tz, refresh_remote: bool) -> Result<Value, String> {
    if uid.is_empty() || uid.len() > 512 || !uid.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("This Google calendar was disconnected. Reopen the calendar.".into());
    }
    let discovery = discover()?;
    if !discovery["calendars"].as_array().is_some_and(|calendars| {
        calendars
            .iter()
            .any(|calendar| calendar["id"].as_str() == Some(uid))
    }) {
        return Err("This Google calendar was disconnected. Reopen the calendar.".into());
    }
    let next = if month.month() == 12 {
        NaiveDate::from_ymd_opt(month.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(month.year(), month.month() + 1, 1)
    }
    .ok_or("Invalid calendar month")?;
    let lo = tz
        .from_local_datetime(&month.and_time(NaiveTime::MIN))
        .earliest()
        .ok_or("Invalid calendar month")?
        .with_timezone(&Utc);
    let hi = tz
        .from_local_datetime(&next.and_time(NaiveTime::MIN))
        .earliest()
        .ok_or("Invalid calendar month")?
        .with_timezone(&Utc);
    let query = CString::new(format!(
        "(or (occur-in-time-range? (make-time \"{}\") (make-time \"{}\")) (has-recurrences?))",
        lo.format("%Y%m%dT%H%M%SZ"),
        hi.format("%Y%m%dT%H%M%SZ")
    ))
    .unwrap();
    let uid = CString::new(uid).map_err(|_| "Invalid calendar source")?;
    // SAFETY: EDS returns owned GObject and GSList references. Every returned
    // list is released after its components have been converted to Rust data.
    unsafe {
        let mut error = ptr::null_mut();
        let timed = TimedCall::new();
        let registry = Object(e_source_registry_new_sync(timed.ptr(), &mut error));
        drop(timed);
        if registry.0.is_null() {
            return Err(fixed_error(error));
        }
        let source = Object(e_source_registry_ref_source(registry.0, uid.as_ptr()));
        if source.0.is_null() || e_source_registry_check_enabled(registry.0, source.0) == 0 {
            return Err("This Google calendar was disconnected. Reopen the calendar.".into());
        }
        error = ptr::null_mut();
        let timed = TimedCall::new();
        let client = Object(e_cal_client_connect_sync(
            source.0,
            0,
            5,
            timed.ptr(),
            &mut error,
        ));
        drop(timed);
        if client.0.is_null() {
            return Err(fixed_error(error));
        }
        let online = e_client_is_online(client.0) != 0;
        if refresh_remote && online {
            error = ptr::null_mut();
            let timed = TimedCall::new();
            let refreshed = e_client_refresh_sync(client.0, timed.ptr(), &mut error);
            drop(timed);
            if refreshed == 0 && !error.is_null() {
                g_error_free(error);
            }
        }
        let mut base = ptr::null_mut();
        error = ptr::null_mut();
        let timed = TimedCall::new();
        let loaded = e_cal_client_get_object_list_sync(
            client.0,
            query.as_ptr(),
            &mut base,
            timed.ptr(),
            &mut error,
        );
        drop(timed);
        if loaded == 0 {
            return Err(fixed_error(error));
        }
        let base = ObjectList(base);
        let mut entries = Vec::new();
        let mut seen = BTreeSet::new();
        let mut fetched = BTreeSet::new();
        let mut size = 0;
        let mut cursor = base.0;
        let mut scanned = 0;
        while !cursor.is_null() {
            scanned += 1;
            if scanned > MAX_EVENTS || entries.len() >= MAX_EVENTS {
                return Err("The calendar service returned too many or unreadable events.".into());
            }
            let component = (*cursor).data;
            let raw = owned_string(i_cal_component_as_ical_string(component))
                .ok_or("The calendar service returned too many or unreadable events.")?;
            let (event_uid, _, recurring) = series_identity(&raw)
                .ok_or("The calendar service returned too many or unreadable events.")?;
            if recurring && fetched.insert(event_uid.clone()) {
                let event_uid =
                    CString::new(event_uid).map_err(|_| "Invalid calendar event identity")?;
                let mut series = ptr::null_mut();
                error = ptr::null_mut();
                let timed = TimedCall::new();
                let loaded = e_cal_client_get_objects_for_uid_sync(
                    client.0,
                    event_uid.as_ptr(),
                    &mut series,
                    timed.ptr(),
                    &mut error,
                );
                drop(timed);
                if loaded == 0 {
                    return Err(fixed_error(error));
                }
                let series = ObjectList(series);
                let mut item = series.0;
                while !item.is_null() {
                    let inner = e_cal_component_get_icalcomponent((*item).data);
                    if inner.is_null() {
                        return Err("A recurring event could not be read completely. The previous download was kept.".into());
                    }
                    append_component(client.0, inner, &mut entries, &mut seen, &mut size)?;
                    item = (*item).next;
                }
            } else if !recurring {
                append_component(client.0, component, &mut entries, &mut seen, &mut size)?;
            }
            cursor = (*cursor).next;
        }
        let result = json!({"events":entries,"accessRole":if e_client_is_readonly(client.0)!=0 {"reader"} else {"writer"},"online":online});
        if serde_json::to_vec(&result).is_ok_and(|bytes| bytes.len() > MAX_RESPONSE) {
            return Err(
                "This calendar exceeds the supported cache size. Previous events were kept.".into(),
            );
        }
        Ok(result)
    }
}

const CONFLICT_FAIL_NO_ITIP: c_int = (1 << 0) | (1 << 4);
const MODIFY_THIS: c_int = 1;
const MODIFY_ALL: c_int = 7;

/// Native write connection. Every operation still requires an explicit,
/// separately verified caller request; constructing this never changes data.
pub struct WriteClient {
    client: Object,
}
impl WriteClient {
    pub fn connect(uid: &str) -> Result<Self, String> {
        let discovery = discover()?;
        if !discovery["calendars"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["id"].as_str() == Some(uid)))
        {
            return Err("This Google calendar was disconnected. Reopen the calendar.".into());
        }
        let uid = CString::new(uid).map_err(|_| "Invalid calendar source")?;
        // SAFETY: EDS constructors return full object references; each call is
        // cancellable and all temporary references are dropped after connect.
        unsafe {
            let mut error = ptr::null_mut();
            let timed = TimedCall::new();
            let registry = Object(e_source_registry_new_sync(timed.ptr(), &mut error));
            drop(timed);
            if registry.0.is_null() {
                return Err(fixed_error(error));
            }
            let source = Object(e_source_registry_ref_source(registry.0, uid.as_ptr()));
            if source.0.is_null() || e_source_registry_check_enabled(registry.0, source.0) == 0 {
                return Err("This Google calendar was disconnected. Reopen the calendar.".into());
            }
            error = ptr::null_mut();
            let timed = TimedCall::new();
            let client = Object(e_cal_client_connect_sync(
                source.0,
                0,
                5,
                timed.ptr(),
                &mut error,
            ));
            drop(timed);
            if client.0.is_null() {
                return Err(fixed_error(error));
            }
            if e_client_is_online(client.0) == 0 || e_client_is_readonly(client.0) != 0 {
                return Err("Connect to the network and choose a writable Google calendar.".into());
            }
            Ok(Self { client })
        }
    }

    pub fn current(&self, uid: &str, rid: &str) -> Result<String, String> {
        let uid = CString::new(uid).map_err(|_| "Invalid event identity")?;
        let rid = if rid.is_empty() {
            None
        } else {
            Some(CString::new(rid).map_err(|_| "Invalid event identity")?)
        };
        // SAFETY: EDS owns returned ICalComponent until Object drops it.
        unsafe {
            let mut error = ptr::null_mut();
            let timed = TimedCall::new();
            let refreshed = e_client_refresh_sync(self.client.0, timed.ptr(), &mut error);
            drop(timed);
            if refreshed == 0 {
                return Err(fixed_error(error));
            }
            let mut component = ptr::null_mut();
            error = ptr::null_mut();
            let timed = TimedCall::new();
            let found = e_cal_client_get_object_sync(
                self.client.0,
                uid.as_ptr(),
                rid.as_ref().map_or(ptr::null(), |value| value.as_ptr()),
                &mut component,
                timed.ptr(),
                &mut error,
            );
            drop(timed);
            let component = Object(component);
            if found == 0 || component.0.is_null() {
                if !error.is_null() {
                    g_error_free(error);
                }
                return Err("This event no longer exists. Sync the calendar.".into());
            }
            let raw = owned_string(i_cal_component_as_ical_string(component.0))
                .ok_or_else(|| "This event no longer exists. Sync the calendar.".to_owned())?;
            select_event_text(
                &raw,
                uid.as_c_str().to_str().unwrap_or(""),
                rid.as_ref()
                    .and_then(|value| value.as_c_str().to_str().ok())
                    .unwrap_or(""),
            )
        }
    }

    pub fn create(&self, ical: &str) -> Result<String, String> {
        let ical = CString::new(ical)
            .map_err(|_| "The event could not be created. Sync before retrying.")?;
        // SAFETY: libical owns the parsed GObject reference until Object drops.
        unsafe {
            let component = Object(i_cal_component_new_from_string(ical.as_ptr()));
            if component.0.is_null() {
                return Err("The event could not be created. Sync before retrying.".into());
            }
            let mut out_uid = ptr::null_mut();
            let mut error = ptr::null_mut();
            let timed = TimedCall::new();
            let saved = e_cal_client_create_object_sync(
                self.client.0,
                component.0,
                CONFLICT_FAIL_NO_ITIP,
                &mut out_uid,
                timed.ptr(),
                &mut error,
            );
            drop(timed);
            let returned = owned_string(out_uid).unwrap_or_default();
            if saved == 0 {
                if !error.is_null() {
                    g_error_free(error);
                }
                return Err("The event could not be created. Sync before retrying.".into());
            }
            Ok(returned)
        }
    }

    pub fn update(&self, ical: &str, recurring: bool) -> Result<(), String> {
        let ical = CString::new(ical).map_err(
            |_| "The event changed elsewhere or could not be saved. Sync before retrying.",
        )?;
        // SAFETY: libical owns the parsed GObject reference until Object drops.
        unsafe {
            let component = Object(i_cal_component_new_from_string(ical.as_ptr()));
            if component.0.is_null() {
                return Err(
                    "The event changed elsewhere or could not be saved. Sync before retrying."
                        .into(),
                );
            }
            let mut error = ptr::null_mut();
            let timed = TimedCall::new();
            let saved = e_cal_client_modify_object_sync(
                self.client.0,
                component.0,
                if recurring { MODIFY_THIS } else { MODIFY_ALL },
                CONFLICT_FAIL_NO_ITIP,
                timed.ptr(),
                &mut error,
            );
            drop(timed);
            if saved == 0 {
                if !error.is_null() {
                    g_error_free(error);
                }
                return Err(
                    "The event changed elsewhere or could not be saved. Sync before retrying."
                        .into(),
                );
            }
            Ok(())
        }
    }

    pub fn delete(&self, uid: &str, rid: &str) -> Result<(), String> {
        let uid = CString::new(uid).map_err(|_| "Invalid event identity")?;
        let rid = if rid.is_empty() {
            None
        } else {
            Some(CString::new(rid).map_err(|_| "Invalid event identity")?)
        };
        // SAFETY: EDS only borrows these input strings during the sync call.
        unsafe {
            let mut error = ptr::null_mut();
            let timed = TimedCall::new();
            let deleted = e_cal_client_remove_object_sync(
                self.client.0,
                uid.as_ptr(),
                rid.as_ref().map_or(ptr::null(), |value| value.as_ptr()),
                if rid.is_some() {
                    MODIFY_THIS
                } else {
                    MODIFY_ALL
                },
                CONFLICT_FAIL_NO_ITIP,
                timed.ptr(),
                &mut error,
            );
            drop(timed);
            if deleted == 0 {
                if !error.is_null() {
                    g_error_free(error);
                }
                return Err(
                    "The event changed elsewhere or could not be deleted. Sync before retrying."
                        .into(),
                );
            }
            Ok(())
        }
    }
}

/// Retain access roles for unchanged source/account pairs in a saved manifest.
pub fn preserve_access_roles(previous: &Value, current: &mut Value) {
    let old: BTreeMap<(&str, &str), &str> = previous["calendars"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some((
                (item["accountId"].as_str()?, item["id"].as_str()?),
                item["accessRole"].as_str()?,
            ))
        })
        .collect();
    if let Some(calendars) = current["calendars"].as_array_mut() {
        for item in calendars {
            if let (Some(account), Some(id)) = (item["accountId"].as_str(), item["id"].as_str()) {
                if let Some(role) = old.get(&(account, id)) {
                    item["accessRole"] = json!(role);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patched_event_is_accepted_by_native_ical_parser() {
        let original = "BEGIN:VEVENT\nUID:fixture\nDTSTART:20261008T080000Z\nDTEND:20261008T090000Z\nX-EVOLUTION-CALDAV-ETAG:version-one\nBEGIN:VALARM\nACTION:DISPLAY\nTRIGGER:-PT15M\nEND:VALARM\nEND:VEVENT\n";
        let fields = json!({"title":"Updated","notes":"Note","location":"Room","allDay":false,"start":"2026-10-08T11:00","end":"2026-10-08T12:00"});
        let edited =
            crate::google_edit::patch_event(original, &fields, chrono_tz::Europe::London, None)
                .unwrap();
        let input = CString::new(edited).unwrap();
        // SAFETY: The parser returns a full GObject reference on success.
        let component = Object(unsafe { i_cal_component_new_from_string(input.as_ptr()) });
        assert!(!component.0.is_null());
    }
    #[cfg(feature = "color")]
    #[test]
    fn calendar_colors_are_bounded_to_rgb_hex() {
        assert_eq!(bounded_color("#aabbccdd"), "#aabbcc");
        assert_eq!(bounded_color("red"), "");
    }
    #[test]
    fn recurrence_identity_is_extracted_from_native_event_text() {
        let raw = "BEGIN:VEVENT\nUID:fixture\nRECURRENCE-ID;TZID=Europe/London:20261008T090000\nDTSTART;TZID=Europe/London:20261008T120000\nEND:VEVENT\n";
        assert_eq!(
            series_identity(raw),
            Some(("fixture".into(), "20261008T090000".into(), true))
        );
        assert_eq!(series_identity("not an event"), None);
    }
    #[test]
    fn selects_exact_event_from_native_master_calendar() {
        let raw = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:fixture\r\nDTSTART:20261008T080000Z\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:fixture\r\nRECURRENCE-ID:20261009T080000Z\r\nDTSTART:20261009T090000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let master = select_event_text(raw, "fixture", "").unwrap();
        assert!(master.contains("DTSTART:20261008T080000Z"));
        assert!(!master.contains("RECURRENCE-ID"));
        let detached = select_event_text(raw, "fixture", "20261009T080000Z").unwrap();
        assert!(detached.contains("DTSTART:20261009T090000Z"));
        assert!(select_event_text(raw, "fixture", "20261010T080000Z").is_err());
    }
    #[test]
    fn access_roles_follow_exact_account_and_source_pair() {
        let old = json!({"calendars":[{"accountId":"one","id":"shared","accessRole":"writer"}]});
        let mut changed = json!({"calendars":[
            {"accountId":"one","id":"shared","accessRole":"unknown"},
            {"accountId":"two","id":"shared","accessRole":"unknown"}]});
        preserve_access_roles(&old, &mut changed);
        assert_eq!(changed["calendars"][0]["accessRole"], "writer");
        assert_eq!(changed["calendars"][1]["accessRole"], "unknown");
    }
}
