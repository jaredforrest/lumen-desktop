//! The same per-user palette state as the desktop shell, without a Python watcher.
use gtk::prelude::*;
use serde_json::{Value,json};
use std::fs;
use std::path::PathBuf;
use std::ffi::{CString,c_char,c_int,c_void};
use std::os::unix::ffi::OsStrExt;

#[link(name="dl")]
unsafe extern "C" {
    fn dlopen(path:*const c_char,flags:c_int)->*mut c_void;
    fn dlsym(handle:*mut c_void,name:*const c_char)->*mut c_void;
    fn dlclose(handle:*mut c_void)->c_int;
}

const TEMPLATE:&str=r#"
window { background: transparent; }
#panel { background: BACKGROUND; border: 1px solid BORDER; border-radius: 16px; color: FOREGROUND; }
#panel * { font-family: "FONT", sans-serif; font-size: 13px; }
#panel button { background: transparent; border: 0; box-shadow: none; text-shadow: none; border-radius: 6px; padding: 0; min-height: 0; min-width: 0; color: FOREGROUND; }
#panel button:hover, #panel button:checked { background: HOVER; }
#panel button:focus { box-shadow: inset 0 0 0 1px ACCENT; }
#panel button.emoji, #panel button.character { min-width: 36px; min-height: 37px; }
#panel button.character label { font-family: "FONT", sans-serif; font-size: 23px; }
#panel button.emoji label { font-family: "EMOJI_FAMILY", "Noto Color Emoji"; font-size: 25px; }
#panel entry { background: SEARCH; color: FOREGROUND; border: 1px solid BORDER; border-radius: 7px; box-shadow: none; min-height: 28px; padding: 0 8px; }
#panel entry:focus { border-color: ACCENT; }
#panel .category { min-width: 29px; min-height: 26px; }
#panel .category label { font-family: sans-serif; font-size: 19px; }
#panel .heading { color: MUTED; font-size: 12px; font-weight: 600; }
#panel .sidebar { background: SEARCH; border-right: 1px solid BORDER; padding: 10px; }
#panel .sidebar button { padding: 7px 8px; }
#panel .sidebar button:checked { background: ACCENT; color: white; }
#panel .details { border-left: 1px solid BORDER; padding: 12px; }
#panel .preview { font-family: "EMOJI_FAMILY", sans-serif; font-size: 64px; }
#panel .text-preview { font-family: "FONT", sans-serif; font-size: 56px; }
#panel .detail-name { font-weight: 600; }
#panel .action { background: SEARCH; border: 1px solid BORDER; padding: 5px 8px; }
#panel .emoji.selected, #panel .character.selected { background: HOVER; box-shadow: inset 0 0 0 1px ACCENT; }
#character-options { background: BACKGROUND; color: FOREGROUND; border: 1px solid BORDER; border-radius: 10px; }
#character-options * { font-family: "FONT", sans-serif; font-size: 12px; }
#character-options button { background: transparent; color: FOREGROUND; border: 0; box-shadow: none; padding: 5px; border-radius: 5px; }
#character-options button:hover, #character-options button:focus { background: HOVER; }
#character-options .emoji label { font-family: "EMOJI_FAMILY", "Noto Color Emoji"; font-size: 25px; }
#character-options .emoji { min-width: 32px; min-height: 34px; padding: 0; }
tooltip { background: BACKGROUND; color: FOREGROUND; border: 1px solid BORDER; border-radius: 6px; }
tooltip label { font-family: "FONT", sans-serif; font-size: 11px; }
#panel .hint { color: MUTED; font-size: 11px; }
#panel .toolbar { border-top: 1px solid BORDER; padding: 7px 6px; }
#panel flowboxchild { padding: 0; margin: 0; }
#panel scrollbar slider { min-width: 4px; background: MUTED; border: 0; }
#panel scrollbar { background: transparent; }
"#;

fn state_path()->PathBuf {
    if let Some(path)=std::env::var_os("LUMEN_APPEARANCE_STATE") {return path.into();}
    let base=std::env::var_os("XDG_STATE_HOME").map(PathBuf::from)
        .unwrap_or_else(||PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    base.join("lumen-desktop/appearance.json")
}
fn read_json(path:&PathBuf)->Option<Value> {
    let bytes=fs::read(path).ok()?;
    if bytes.len()>1024*1024 {return None;}
    serde_json::from_slice(&bytes).ok()
}
fn fallback()->Value {
    std::env::var_os("CHARACTER_VIEWER_PALETTE").and_then(|path|read_json(&PathBuf::from(path)))
        .unwrap_or_else(||json!({"font":"Interlude Variable","light":{
            "background":"#f2f2f4","text":"#202023","border":"#c8c8ce","hover":"#dedee3",
            "field":"#e5e5e8","muted":"#626267","accent":"#007aff"}}))
}
fn color(value:&Value,key:&str,default:&str)->String {
    let Some(raw)=value[key].as_str() else {return default.into()};
    if raw.len()==7 && raw.starts_with('#') && raw[1..].bytes().all(|byte|byte.is_ascii_hexdigit()) {
        raw.to_owned()
    } else {default.into()}
}
fn safe_font(value:Option<&str>,default:&str)->String {
    value.filter(|value|value.len()<=100 && value.chars().all(|ch|ch.is_alphanumeric() || ch==' ' || ch=='-' || ch=='_'))
        .unwrap_or(default).to_owned()
}
fn render(state:&Value,defaults:&Value)->String {
    let palette=state.get("palette").filter(|value|value.is_object()).unwrap_or(&defaults["light"]);
    let font=safe_font(state["font"].as_str(),defaults["font"].as_str().unwrap_or("sans-serif"));
    let emoji=safe_font(std::env::var("LUMEN_EMOJI_FONT_FAMILY").ok().as_deref(),
        "Noto Color Emoji");
    let mut css=TEMPLATE.to_owned();
    for (token,key,default) in [("BACKGROUND","background","#f2f2f4"),("FOREGROUND","text","#202023"),
        ("BORDER","border","#c8c8ce"),("HOVER","hover","#dedee3"),
        ("SEARCH","field","#e5e5e8"),("MUTED","muted","#626267"),("ACCENT","accent","#007aff")] {
        css=css.replace(token,&color(palette,key,default));
    }
    css.replace("EMOJI_FAMILY",&emoji).replace("FONT",&font)
}

pub fn install() {
    let Some(screen)=gtk::gdk::Screen::default() else {return};
    let provider=gtk::CssProvider::new();
    gtk::StyleContext::add_provider_for_screen(&screen,&provider,gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    let defaults=fallback();let path=state_path();
    let mut last=String::new();
    let mut refresh=move || {
        let value=read_json(&path).unwrap_or_else(||defaults.clone());
        let serialized=value.to_string();
        if serialized!=last {
            let css=render(&value,&defaults);
            if let Err(error)=provider.load_from_data(css.as_bytes()) {eprintln!("Viewer appearance: {error}");}
            if let Some(settings)=gtk::Settings::default() {
                let dark=value["mode"]=="dark";
                settings.set_gtk_application_prefer_dark_theme(dark);
                settings.set_gtk_theme_name(Some(if dark {"WhiteSur-Dark"} else {"WhiteSur-Light"}));
            }
            last=serialized;
        }
    };
    refresh();
    gtk::glib::timeout_add_seconds_local(1,move || {refresh();gtk::glib::ControlFlow::Continue});
}

pub fn register_font() {
    let (Some(library),Some(font))=(std::env::var_os("CHARACTER_VIEWER_FONTCONFIG"),
        std::env::var_os("CHARACTER_VIEWER_FONT")) else {return};
    let (Ok(library),Ok(font))=(CString::new(library.as_bytes()),CString::new(font.as_bytes())) else {return};
    unsafe {
        let handle=dlopen(library.as_ptr(),2);
        if handle.is_null(){eprintln!("Character viewer fontconfig library is unavailable");return;}
        let current=dlsym(handle,c"FcConfigGetCurrent".as_ptr());
        let add=dlsym(handle,c"FcConfigAppFontAddFile".as_ptr());
        if !current.is_null() && !add.is_null() {
            let current:unsafe extern "C" fn()->*mut c_void=std::mem::transmute(current);
            let add:unsafe extern "C" fn(*mut c_void,*const u8)->c_int=std::mem::transmute(add);
            if add(current(),font.as_ptr().cast())==0 {eprintln!("Character viewer emoji font could not be registered");}
        }
        dlclose(handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_palette_tokens_cannot_inject_css() {
        let defaults=fallback();
        let css=render(&json!({"palette":{"background":"red;} button {color:red"},"font":"bad\"; * {"}),&defaults);
        assert!(!css.contains("bad\""));
        assert!(!css.contains("red;}"));
        assert!(css.contains("#f2f2f4"));
    }
}
