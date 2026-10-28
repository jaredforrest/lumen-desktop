//! Explicit text abbreviation expansion for the character viewer input method.
use serde_json::Value;
use std::env;
use std::fs;
use std::path::PathBuf;

const DESKTOP: &str = "lumen-desktop";

fn preferences() -> Option<PathBuf> {
    let root=env::var_os("XDG_STATE_HOME").map(PathBuf::from)
        .or_else(||env::var_os("HOME").map(|home|PathBuf::from(home).join(".local/state")))?;
    Some(root.join(DESKTOP).join("preferences"))
}

fn truthy(value:&Value) -> bool {
    match value {
        Value::Null=>false,Value::Bool(value)=>*value,
        Value::Number(value)=>value.as_f64().is_some_and(|value|value!=0.),
        Value::String(value)=>!value.is_empty(),Value::Array(value)=>!value.is_empty(),Value::Object(value)=>!value.is_empty(),
    }
}

pub fn replacement_for(prefix:&str) -> Option<(usize,String)> {
    let folder=preferences()?;
    match fs::read(folder.join("text-tools-options.json")) {
        Ok(bytes) => {
            let options:Value=serde_json::from_slice(&bytes).ok()?;
            if !options.is_object() || options.get("automaticReplacements").is_some_and(|value|!truthy(value)) {return None;}
        }
        Err(error) if error.kind()==std::io::ErrorKind::NotFound => (),
        Err(_) => return None,
    }
    let space=prefix.chars().next_back().filter(|ch|matches!(ch,' '| '\t'|'\n'))?;
    let rows:Value=serde_json::from_slice(&fs::read(folder.join("text-tools.json")).ok()?).ok()?;
    let rows=rows.as_array()?;
    let mut choices=rows.iter().take(100).filter_map(|row|Some((row.get("name")?.as_str()?,row.get("text")?.as_str()?)))
        .collect::<Vec<_>>();
    choices.sort_by_key(|(name,_)|std::cmp::Reverse(name.chars().count()));
    for (name,text) in choices {
        if name.is_empty() || name.chars().count()>40 || text.is_empty() || text.chars().count()>8192 || name.chars().any(char::is_whitespace) {continue;}
        let ending=format!("{name}{space}");
        let Some(before)=prefix.strip_suffix(&ending) else {continue};
        if before.chars().next_back().is_some_and(|ch|ch.is_alphanumeric() || ch=='_'){continue;}
        return Some((ending.len(),format!("{text}{space}")));
    }
    None
}

/// Return the output byte length, or zero when no safe replacement exists.
/// The caller owns `output` and provides its capacity; all pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cv_replacement_for(prefix:*const u8,prefix_len:usize,output:*mut u8,
    capacity:usize,delete_bytes:*mut u32) -> usize {
    if prefix.is_null() || output.is_null() || delete_bytes.is_null() || prefix_len>1_048_576 || capacity>1_048_576 {return 0;}
    let input=unsafe {std::slice::from_raw_parts(prefix,prefix_len)};
    let Ok(input)=std::str::from_utf8(input) else {return 0};
    let Some((count,value))=replacement_for(input) else {return 0};
    if count>u32::MAX as usize || value.len()>capacity {return 0;}
    unsafe {
        std::ptr::copy_nonoverlapping(value.as_ptr(),output,value.len());
        *delete_bytes=count as u32;
    }
    value.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundary_and_explicit_disable() {
        let root=env::temp_dir().join(format!("character-replacements-{}",std::process::id()));
        let folder=root.join("lumen-desktop/preferences");fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("text-tools.json"),r#"[{"name":"omw","text":"On my way!"},{"name":"café","text":"coffee"},{"name":"two words","text":"ignored"}]"#).unwrap();
        unsafe {env::set_var("XDG_STATE_HOME",&root);}
        assert_eq!(replacement_for("Hello, omw "),Some((4,"On my way! ".into())));
        assert_eq!(replacement_for("café "),Some((6,"coffee ".into())));
        for text in ["someomw ","_omw ","omw","omw.","omw next "] {assert_eq!(replacement_for(text),None);}
        fs::write(folder.join("text-tools-options.json"),r#"{"automaticReplacements":false}"#).unwrap();
        assert_eq!(replacement_for("omw "),None);
        fs::remove_file(folder.join("text-tools-options.json")).unwrap();
        fs::write(folder.join("text-tools.json"),r#"[null,{"name":5,"text":"wrong"},{"name":"omw","text":""},{"name":"two words","text":"skip"}]"#).unwrap();
        assert_eq!(replacement_for("two words "),None);
        assert_eq!(replacement_for("omw "),None);
        fs::remove_dir_all(root).unwrap();
    }
}
