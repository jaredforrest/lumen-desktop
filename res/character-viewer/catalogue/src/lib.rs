//! Unicode character catalogue shared by the compact and expanded viewer.
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::fs;
use unicode_general_category::{GeneralCategory, get_general_category};

pub fn base_key(text: &str) -> String {
    let key: String = text.chars().filter(|ch| !matches!(*ch as u32,0x1f3fb..=0x1f3ff|0xfe0f)).collect();
    if key == "🫱\u{200d}🫲" { "🤝".into() } else { key }
}

fn group_name(value: &str) -> &str {
    match value {
        "Smileys & Emotion" | "People & Body" => "Smileys & People",
        _ => value,
    }
}

fn emoji_item(line: &str, group: &str) -> Option<Value> {
    let (codes,description) = line.split_once(';')?;
    let description = description.trim();
    if !description.starts_with("fully-qualified") || group == "Component" { return None; }
    let (_,comment) = description.split_once('#')?;
    let mut parts = comment.split_whitespace();
    parts.next()?;
    let version = parts.next()?;
    if !version.starts_with('E') || !version[1..].chars().all(|ch| ch.is_ascii_digit() || ch=='.') { return None; }
    let name = parts.collect::<Vec<_>>().join(" ");
    if name.is_empty() { return None; }
    let text: String = codes.split_whitespace().map(|code| u32::from_str_radix(code,16).ok().and_then(char::from_u32))
        .collect::<Option<String>>()?;
    Some(json!({"text":text,"name":name,"group":group}))
}

fn skipped_category(ch: char) -> bool {
    matches!(get_general_category(ch),GeneralCategory::Control|GeneralCategory::Format|
        GeneralCategory::PrivateUse|GeneralCategory::Surrogate|GeneralCategory::Unassigned|
        GeneralCategory::SpaceSeparator|GeneralCategory::LineSeparator|GeneralCategory::ParagraphSeparator)
}

fn add_symbol(ch: char, section: &str, records: &mut Vec<Value>, added: &mut HashSet<String>) {
    let text = ch.to_string();
    if skipped_category(ch) || !added.insert(text.clone()) { return; }
    let name = unicode_names2::name(ch).map(|name| name.to_string().to_lowercase()).unwrap_or_else(||text.clone());
    records.push(json!({"text":text,"name":name,"group":"Symbols","section":section,"symbol":true}));
}

pub fn catalogue(source: &str) -> Value {
    let mut records = Vec::new();
    let mut variants: HashMap<String,Vec<Value>> = HashMap::new();
    let mut group = String::new();
    for line in source.lines() {
        if let Some(value) = line.strip_prefix("# group: ") { group=group_name(value).to_owned(); }
        let Some(item) = emoji_item(line,&group) else { continue };
        let text = item["text"].as_str().unwrap();
        variants.entry(base_key(text)).or_default().push(item.clone());
        if !text.chars().any(|ch| matches!(ch as u32,0x1f3fb..=0x1f3ff)) { records.push(item); }
    }
    let mut added = HashSet::new();
    for (section,ranges) in [
        ("Arrows",&[(0x2190,0x2200)][..]),
        ("Currency Symbols",&[(0x20a0,0x20c1)][..]),
        ("Latin",&[(0x00c0,0x0250)][..]),
        ("Letterlike Symbols",&[(0x2100,0x2150)][..]),
        ("Maths Symbols",&[(0x2200,0x2300)][..]),
        ("Punctuation",&[(0x2000,0x2070)][..]),
    ] {
        for &(start,end) in ranges {
            for code in start..end {
                if let Some(ch)=char::from_u32(code) { add_symbol(ch,section,&mut records,&mut added); }
            }
        }
    }
    for ch in "⌘⌥⌃⇧⇪⎋⇥↩⌫⌦⏎©®™°±×÷πµ∆€£¥¢₩₹₽₿".chars() {
        let section = if get_general_category(ch)==GeneralCategory::CurrencySymbol {"Currency Symbols"} else {"Letterlike Symbols"};
        add_symbol(ch,section,&mut records,&mut added);
    }
    json!({"records":records,"variants":variants})
}

pub fn render_sections(data: &Value, state: &Value, section: &str, query: &str, expanded: bool) -> Value {
    let Some(records)=data["records"].as_array() else { return json!([]); };
    let mut lookup: HashMap<&str,&Value> = HashMap::new();
    if let Some(variants)=data["variants"].as_object() {
        for items in variants.values() {
            if let Some(items)=items.as_array() {
                for item in items { if let Some(text)=item["text"].as_str() { lookup.insert(text,item); } }
            }
        }
    }
    for item in records { if let Some(text)=item["text"].as_str() { lookup.insert(text,item); } }
    let trimmed=query.trim().to_lowercase();
    let mut sections: Vec<(&str,Vec<Value>)> = Vec::new();
    if !trimmed.is_empty() {
        let normalized=trimmed.replace(['_','-']," ");
        let words: Vec<&str>=normalized.split_whitespace().collect();
        let items=records.iter().filter(|item| {
            let text=item["text"].as_str().unwrap_or("");
            let codes=text.chars().map(|ch|format!("{:04x}",ch as u32)).collect::<Vec<_>>().join(" ");
            let haystack=format!("{} {text} {codes}",item["name"].as_str().unwrap_or("")).to_lowercase().replace('-'," ");
            words.iter().all(|word|haystack.contains(word.strip_prefix("u+").unwrap_or(word)))
        }).cloned().collect();
        sections.push(("Search Results",items));
    } else if section=="Recent" {
        let mut recent:Vec<Value>=state["recent"].as_array().into_iter().flatten()
            .filter_map(|text|text.as_str().and_then(|text|lookup.get(text)).map(|item|(*item).clone())).collect();
        if !expanded { recent.truncate(16); }
        if !recent.is_empty() { sections.push(("Recently Used",recent)); }
        sections.push(("Smileys & People",records.iter().filter(|item|item["group"]=="Smileys & People").cloned().collect()));
    } else if section=="Favorites" {
        sections.push(("Favourites",state["favorites"].as_array().into_iter().flatten()
            .filter_map(|text|text.as_str().and_then(|text|lookup.get(text)).map(|item|(*item).clone())).collect()));
    } else {
        sections.push((section,records.iter().filter(|item|item["group"]==section || item["section"]==section).cloned().collect()));
    }
    Value::Array(sections.into_iter().map(|(title,items)| {
        let items:Vec<Value>=if title=="Recently Used" || title=="Favourites" { items } else {
            items.into_iter().map(|item| {
                let text=item["text"].as_str().unwrap_or("");
                let selected=state["tones"][base_key(text)].as_str().and_then(|text|lookup.get(text));
                selected.map(|item|(*item).clone()).unwrap_or(item)
            }).collect()
        };
        json!({"title":title,"items":items})
    }).collect())
}

fn filtered_state(data: &Value) -> Value {
    let mut state = serde_json::Map::new();
    let Some(fields) = data.as_object() else { return json!({}); };
    for (key,value) in fields {
        let valid = match key.as_str() {
            "recent" | "favorites" => value.as_array().is_some_and(|items| items.iter().all(Value::is_string)),
            "expanded" => value.is_boolean(),
            "tones" => value.as_object().is_some_and(|tones| tones.values().all(Value::is_string)),
            _ => false,
        };
        if valid { state.insert(key.clone(),value.clone()); }
    }
    Value::Object(state)
}

pub fn load_state(path: &Path) -> Value {
    fs::read(path).ok().filter(|raw| raw.len()<=1024*1024)
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .map(|value| filtered_state(&value)).unwrap_or_else(||json!({}))
}

pub fn save_state(path: &Path, value: &Value) -> io::Result<()> {
    if !value.is_object() { return Err(io::Error::other("Invalid character viewer state")); }
    let parent = path.parent().ok_or_else(||io::Error::other("Invalid character viewer state path"))?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent,fs::Permissions::from_mode(0o700))?;
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    for attempt in 0..16 {
        let temporary=parent.join(format!(".state-{}-{attempt}.tmp",std::process::id()));
        let mut file=match fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary) {
            Ok(file)=>file,
            Err(error) if error.kind()==io::ErrorKind::AlreadyExists=>continue,
            Err(error)=>return Err(error),
        };
        let result=(|| {
            file.write_all(&bytes)?;file.sync_all()?;
            fs::rename(&temporary,path)?;
            fs::File::open(parent)?.sync_all()
        })();
        if result.is_err() { let _=fs::remove_file(&temporary); }
        return result;
    }
    Err(io::Error::other("Character viewer state is busy"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tone_keys_and_qualification() {
        assert_eq!(base_key("👋🏽️"),"👋");
        assert_eq!(base_key("🫱🏽\u{200d}🫲🏻"),"🤝");
        assert!(emoji_item("1F600 ; fully-qualified # 😀 E1.0 grinning face","Smileys & People").is_some());
        assert!(emoji_item("1F600 ; minimally-qualified # 😀 E1.0 grinning face","Smileys & People").is_none());
    }
    #[test]
    fn catalogue_preserves_emoji_and_symbol_sections() {
        let result=catalogue("# group: Smileys & Emotion\n1F600 ; fully-qualified # 😀 E1.0 grinning face\n");
        assert_eq!(result["records"][0]["group"],"Smileys & People");
        assert!(result["records"].as_array().unwrap().iter().any(|item|item["section"]=="Arrows"));
    }
    #[test]
    fn state_load_keeps_only_valid_fields() {
        let data=json!({"recent":["👋"],"favorites":[1],"expanded":true,
            "tones":{"👋":"👋🏽"},"unknown":"discard"});
        assert_eq!(filtered_state(&data),json!({"recent":["👋"],"expanded":true,"tones":{"👋":"👋🏽"}}));
    }
    #[test]
    fn sections_preserve_recent_favorites_search_and_tones() {
        let data=catalogue("# group: Smileys & Emotion\n1F44B ; fully-qualified # 👋 E1.0 waving hand\n1F44B 1F3FD ; fully-qualified # 👋🏽 E1.0 waving hand: medium skin tone\n");
        let state=json!({"recent":["👋"],"favorites":["👋"],"tones":{"👋":"👋🏽"}});
        assert_eq!(render_sections(&data,&state,"Recent","",false)[0]["items"][0]["text"],"👋");
        assert_eq!(render_sections(&data,&state,"Recent","",false)[1]["items"][0]["text"],"👋🏽");
        assert_eq!(render_sections(&data,&state,"Favorites","",false)[0]["items"][0]["text"],"👋");
        assert_eq!(render_sections(&data,&state,"Recent","u+1f44b",false)[0]["items"][0]["text"],"👋🏽");
    }
}
