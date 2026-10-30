//! Read-only translation uses the same raw/translated and consumed-mask paths as
//! the pinned compositor. No text entry, key events, or compositor commands.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

#[link(name = "xkbcommon")]
unsafe extern "C" {
    fn xkb_context_new(flags: u32) -> *mut c_void;
    fn xkb_context_unref(context: *mut c_void);
    #[cfg(test)]
    fn xkb_context_include_path_append(context: *mut c_void, path: *const c_char) -> c_int;
    fn xkb_keymap_new_from_string(
        context: *mut c_void,
        text: *const c_char,
        format: u32,
        flags: u32,
    ) -> *mut c_void;
    fn xkb_keymap_unref(map: *mut c_void);
    fn xkb_keymap_min_keycode(map: *mut c_void) -> u32;
    fn xkb_keymap_max_keycode(map: *mut c_void) -> u32;
    fn xkb_keymap_num_layouts(map: *mut c_void) -> u32;
    fn xkb_keymap_key_get_name(map: *mut c_void, code: u32) -> *const c_char;
    fn xkb_keymap_key_get_syms_by_level(
        map: *mut c_void,
        code: u32,
        layout: u32,
        level: u32,
        symbols: *mut *const u32,
    ) -> c_int;
    fn xkb_state_new(map: *mut c_void) -> *mut c_void;
    fn xkb_state_unref(state: *mut c_void);
    fn xkb_state_update_mask(
        state: *mut c_void,
        depressed: u32,
        latched: u32,
        locked: u32,
        depressed_layout: u32,
        latched_layout: u32,
        locked_layout: u32,
    ) -> u32;
    fn xkb_state_key_get_layout(state: *mut c_void, code: u32) -> u32;
    fn xkb_state_key_get_syms(state: *mut c_void, code: u32, symbols: *mut *const u32) -> c_int;
    fn xkb_state_key_get_consumed_mods2(state: *mut c_void, code: u32, mode: u32) -> u32;
    fn xkb_keysym_to_utf8(symbol: u32, buffer: *mut c_char, size: usize) -> c_int;
    fn xkb_keysym_get_name(symbol: u32, buffer: *mut c_char, size: usize) -> c_int;
    fn xkb_keysym_from_name(name: *const c_char, flags: u32) -> u32;
    #[cfg(test)]
    fn xkb_keymap_get_as_string(map: *mut c_void, format: u32) -> *mut c_char;
}
struct Context(*mut c_void);
impl Drop for Context {
    fn drop(&mut self) {
        unsafe { xkb_context_unref(self.0) }
    }
}
struct Map(*mut c_void);
impl Drop for Map {
    fn drop(&mut self) {
        unsafe { xkb_keymap_unref(self.0) }
    }
}
struct State(*mut c_void);
impl Drop for State {
    fn drop(&mut self) {
        unsafe { xkb_state_unref(self.0) }
    }
}
/// The pair is XKB physical keycode (evdev+8), WLR modifier mask.
pub type Activation = (u16, u8);
#[derive(Default)]
pub struct Model {
    symbols: BTreeMap<(u32, u8), BTreeSet<Activation>>,
    codes: BTreeMap<(u32, u8), BTreeSet<Activation>>,
    names: BTreeMap<u16, String>,
}
fn context() -> Result<Context, String> {
    // Expanded production keymaps must not consult the user's environment or includes.
    let value = Context(unsafe { xkb_context_new(1 | 2) });
    if value.0.is_null() {
        Err("Keyboard context unavailable".into())
    } else {
        Ok(value)
    }
}
fn parse(context: &Context, text: &str) -> Result<Map, String> {
    if text.len() > 128 * 1024 {
        return Err("Keyboard keymap limit exceeded".into());
    }
    let text = CString::new(text).map_err(|_| "Invalid keyboard keymap")?;
    let map = Map(unsafe { xkb_keymap_new_from_string(context.0, text.as_ptr(), 1, 0) });
    if map.0.is_null() {
        Err("Keyboard keymap could not be read".into())
    } else {
        Ok(map)
    }
}
fn syms(pointer: *const u32, count: c_int) -> Vec<u32> {
    if pointer.is_null() || !(0..=8).contains(&count) {
        return vec![];
    }
    unsafe { std::slice::from_raw_parts(pointer, count as usize) }.to_vec()
}
impl Model {
    pub fn new(text: &str, layout: u32, indexes: &[u32]) -> Result<Self, String> {
        if indexes.len() != 8 || indexes.iter().any(|i| *i != u32::MAX && *i >= 32) {
            return Err("Invalid keyboard modifier indexes".into());
        }
        let context = context()?;
        let map = parse(&context, text)?;
        let min = unsafe { xkb_keymap_min_keycode(map.0) };
        let max = unsafe { xkb_keymap_max_keycode(map.0) };
        let layouts = unsafe { xkb_keymap_num_layouts(map.0) };
        if max > 1023 || min > max || layouts > 8 || layout >= layouts {
            return Err("Keyboard layout exceeds observation limits".into());
        }
        let state = State(unsafe { xkb_state_new(map.0) });
        if state.0.is_null() {
            return Err("Keyboard state unavailable".into());
        }
        let mut result = Self::default();
        for code in min..=max {
            let name = unsafe { xkb_keymap_key_get_name(map.0, code) };
            if name.is_null() {
                continue;
            }
            let name = unsafe { CStr::from_ptr(name) }
                .to_str()
                .map_err(|_| "Invalid keyboard key name")?;
            if name.len() > 64 {
                return Err("Keyboard key name exceeds limits".into());
            }
            unsafe { xkb_state_update_mask(state.0, 0, 0, 0, 0, 0, layout) };
            let group = unsafe { xkb_state_key_get_layout(state.0, code) };
            let mut base = ptr::null();
            let count =
                unsafe { xkb_keymap_key_get_syms_by_level(map.0, code, group, 0, &mut base) };
            let label = syms(base, count)
                .first()
                .map(|symbol| symbol_name(*symbol))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| name.to_string());
            result.names.insert(code as u16, label);
            for mask in 0..=255u8 {
                let mut xkb_mask = 0;
                for (bit, index) in indexes.iter().enumerate() {
                    if mask & (1 << bit) != 0 && *index != u32::MAX {
                        xkb_mask |= 1u32 << index
                    }
                }
                unsafe { xkb_state_update_mask(state.0, xkb_mask, 0, 0, 0, 0, layout) };
                let group = unsafe { xkb_state_key_get_layout(state.0, code) };
                let mut pointer = ptr::null();
                let count = unsafe {
                    xkb_keymap_key_get_syms_by_level(map.0, code, group, 0, &mut pointer)
                };
                for sym in syms(pointer, count) {
                    result
                        .symbols
                        .entry((sym, mask))
                        .or_default()
                        .insert((code as u16, mask));
                }
                let count = unsafe { xkb_state_key_get_syms(state.0, code, &mut pointer) };
                // Deliberately mirrors the pinned Sway consumed-mask operation.
                let consumed = unsafe { xkb_state_key_get_consumed_mods2(state.0, code, 0) };
                let translated = mask & !(consumed as u8);
                for sym in syms(pointer, count) {
                    result
                        .symbols
                        .entry((sym, translated))
                        .or_default()
                        .insert((code as u16, mask));
                }
                result
                    .codes
                    .entry((code, mask))
                    .or_default()
                    .insert((code as u16, mask));
            }
        }
        Ok(result)
    }
    pub fn activations(&self, kind: &str, key: u32, mask: u8) -> BTreeSet<Activation> {
        if kind == "code" {
            self.codes.get(&(key, mask))
        } else {
            self.symbols.get(&(key, mask))
        }
        .cloned()
        .unwrap_or_default()
    }
    pub fn labels(&self, activations: &BTreeSet<Activation>) -> Vec<String> {
        // Lock and Num Lock variants do not require duplicate keycap previews.
        let mut values = activations
            .iter()
            .filter(|(_, mask)| mask & 18 == 0)
            .map(|(code, mask)| {
                let mut parts = Vec::new();
                for (bit, name) in [
                    (0, "Shift"),
                    (2, "Ctrl"),
                    (3, "Alt"),
                    (5, "Mod3"),
                    (6, "Super"),
                    (7, "AltGr"),
                ] {
                    if mask & (1 << bit) != 0 {
                        parts.push(name.to_string());
                    }
                }
                parts.push(
                    self.names
                        .get(code)
                        .cloned()
                        .unwrap_or_else(|| format!("Code {code}")),
                );
                parts.join("+")
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        values.truncate(8);
        values
    }
}
fn symbol_name(symbol: u32) -> String {
    let mut buffer = [0i8; 128];
    let n = unsafe { xkb_keysym_to_utf8(symbol, buffer.as_mut_ptr(), buffer.len()) };
    if n > 1 {
        let value = unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .to_string();
        if !value.chars().any(char::is_control) {
            return value.to_uppercase();
        }
    }
    let n = unsafe { xkb_keysym_get_name(symbol, buffer.as_mut_ptr(), buffer.len()) };
    if n <= 0 || n >= 128 {
        return String::new();
    }
    let value = unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .to_string();
    match value.as_str() {
        "Return" => "Enter".into(),
        "Escape" => "Esc".into(),
        "space" => "Space".into(),
        other => other.into(),
    }
}
pub fn symbol(name: &str) -> u32 {
    CString::new(name)
        .map(|s| unsafe { xkb_keysym_from_name(s.as_ptr(), 1) })
        .unwrap_or(0)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn keymap(layout: &str) -> String {
        let context = context().unwrap();
        let path = CString::new(env!("LUMEN_XKB_ROOT")).unwrap();
        assert_eq!(
            unsafe { xkb_context_include_path_append(context.0, path.as_ptr()) },
            1
        );
        let source = format!(
            "xkb_keymap {{ xkb_keycodes {{ include \"evdev+aliases(qwerty)\" }}; xkb_types {{ include \"complete\" }}; xkb_compatibility {{ include \"complete\" }}; xkb_symbols {{ include \"pc+{layout}+inet(evdev)\" }}; }};"
        );
        let map = parse(&context, &source).unwrap();
        let text = unsafe { xkb_keymap_get_as_string(map.0, 1) };
        assert!(!text.is_null());
        let value = unsafe { CStr::from_ptr(text) }
            .to_str()
            .unwrap()
            .to_string();
        unsafe { libc::free(text.cast()) };
        value
    }
    #[test]
    fn actual_us_german_layout_changes_physical_collision_without_running_commands() {
        let indexes = [0, 1, 2, 3, 4, 5, 6, 7];
        let us = Model::new(&keymap("us"), 0, &indexes).unwrap();
        let de = Model::new(&keymap("de"), 0, &indexes).unwrap();
        let plus = symbol("plus");
        let equal = symbol("equal");
        let us_plus = us.activations("symbol", plus, 4);
        let us_equal = us.activations("symbol", equal, 5);
        assert!(!us_plus.is_empty());
        assert!(!us_plus.is_disjoint(&us_equal));
        let de_plus = de.activations("symbol", plus, 4);
        let de_equal = de.activations("symbol", equal, 5);
        assert!(!de_plus.is_empty());
        assert!(de_plus.is_disjoint(&de_equal));
        assert_ne!(us.labels(&us_plus), de.labels(&de_plus));
        let code = *us_plus.iter().find(|(_, mask)| *mask == 5).unwrap();
        assert!(
            us.activations("code", code.0.into(), code.1)
                .contains(&code)
        );
    }
    #[test]
    fn synthetic_nonidentity_indexes_preserve_pinned_compositor_consumption() {
        // This is deliberately the pinned compositor's direct mask intersection,
        // not an assumption that XKB modifier indexes universally equal WLR bits.
        let model = Model::new(&keymap("us"), 0, &[5, 1, 2, 3, 4, 0, 6, 7]).unwrap();
        // WLR Mod3 (bit5) now depresses XKB Shift (index0). Sway's direct
        // consumed-mask subtraction clears bit0, not WLR bit5.
        assert!(
            model
                .activations("symbol", symbol("plus"), 32)
                .contains(&(21, 32))
        );
        assert!(
            !model
                .activations("symbol", symbol("plus"), 0)
                .contains(&(21, 32))
        );
    }
    #[test]
    fn malformed_or_unbounded_keymaps_refuse() {
        assert!(Model::new("", 0, &[0; 8]).is_err());
        assert!(Model::new(&"x".repeat(128 * 1024 + 1), 0, &[0; 8]).is_err());
        assert!(Model::new("", 0, &[32; 8]).is_err());
        assert!(Model::new(&keymap("us"), 8, &[0, 1, 2, 3, 4, 5, 6, 7]).is_err());
    }
}
