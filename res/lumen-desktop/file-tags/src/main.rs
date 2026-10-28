//! Versioned colour tags bound to opened inodes and transported over JSON lines.
use serde_json::{Value, json};
use std::ffi::CString;
use std::fs::File;
use std::io::{BufRead, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

const ATTRIBUTE: &str = "user.lumen.tags.v1";
const MAX_RAW: usize = 4096;
const IDS: [&str;7] = ["red","orange","yellow","green","blue","purple","grey"];
const TAGS: [(&str,&str,&str);7] = [
    ("red","Red","#e54b4b"),("orange","Orange","#e79735"),
    ("yellow","Yellow","#d6b82f"),("green","Green","#50a86b"),
    ("blue","Blue","#438ed6"),("purple","Purple","#9b6ac6"),
    ("grey","Grey","#929298")];
type Result<T> = std::result::Result<T,String>;

fn cpath(value: &Path) -> Result<CString> {
    CString::new(value.as_os_str().as_bytes()).map_err(|_| "Invalid file path".into())
}
fn path<'a>(request: &'a Value, key: &str) -> Result<&'a Path> {
    request[key].as_str().filter(|value| !value.is_empty() && !value.contains('\0'))
        .map(Path::new).ok_or_else(|| format!("Invalid {key} path"))
}
fn missing(code: i32) -> bool { code == libc::ENODATA }
fn unsupported(code: i32) -> bool { code == libc::ENOTSUP || code == libc::EOPNOTSUPP }

fn read_raw_path(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let path = cpath(path).map_err(std::io::Error::other)?;
    let attribute = CString::new(ATTRIBUTE).unwrap();
    let mut raw = vec![0_u8;MAX_RAW+1];
    let length = unsafe { libc::lgetxattr(path.as_ptr(),attribute.as_ptr(),raw.as_mut_ptr().cast(),raw.len()) };
    if length < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error().is_some_and(missing) { return Ok(None); }
        return Err(error);
    }
    raw.truncate(length as usize);
    Ok(Some(raw))
}

fn read_raw_fd(file: &File) -> std::io::Result<Option<Vec<u8>>> {
    let attribute = CString::new(ATTRIBUTE).unwrap();
    let mut raw = vec![0_u8;MAX_RAW+1];
    let length = unsafe { libc::fgetxattr(file.as_raw_fd(),attribute.as_ptr(),raw.as_mut_ptr().cast(),raw.len()) };
    if length < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error().is_some_and(missing) { return Ok(None); }
        return Err(error);
    }
    raw.truncate(length as usize);
    Ok(Some(raw))
}

fn validate(value: &Value) -> Result<Vec<String>> {
    let ids = value.as_array().ok_or("Choose distinct supported colour tags")?;
    let mut result = Vec::new();
    for value in ids {
        let id = value.as_str().filter(|id| IDS.contains(id)).ok_or("Choose distinct supported colour tags")?;
        if result.iter().any(|existing| existing == id) { return Err("Choose distinct supported colour tags".into()); }
        result.push(id.to_owned());
    }
    Ok(result)
}

fn decode(raw: &[u8]) -> Result<Vec<String>> {
    if raw.len() > MAX_RAW { return Err("Tag metadata exceeds its size limit".into()); }
    let value: Value = serde_json::from_slice(raw).map_err(|error| error.to_string())?;
    if !value.is_object() || value["version"].as_u64() != Some(1) {
        return Err("Unsupported tag metadata version".into());
    }
    validate(&value["tags"])
}

fn projection(raw: std::io::Result<Option<Vec<u8>>>) -> Value {
    let ids = match raw {
        Ok(None) => return json!({"tags":[],"supported":true,"error":""}),
        Ok(Some(raw)) => match decode(&raw) {
            Ok(ids) => ids,
            Err(error) => return json!({"tags":[],"supported":true,
                "error":format!("Colour tag metadata could not be read: {error}")}),
        },
        Err(error) if error.raw_os_error()==Some(libc::ERANGE) => return json!({"tags":[],"supported":true,
            "error":"Colour tag metadata could not be read: Tag metadata exceeds its size limit"}),
        Err(error) => return json!({"tags":[],"supported":false,
            "error":format!("This item cannot store colour tags: {error}")}),
    };
    json!({"tags":TAGS.iter().filter(|(id,_,_)| ids.iter().any(|value| value == id))
        .map(|(id,label,color)| json!({"id":id,"label":label,"color":color})).collect::<Vec<_>>(),
        "supported":true,"error":""})
}

fn component(value: &Value) -> Result<u64> {
    if let Some(text) = value.as_str() {
        if text.is_empty() || text.len()>20 || (text.len()>1 && text.starts_with('0'))
            || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("Invalid or imprecise file identity".into());
        }
        return text.parse().map_err(|_| "Invalid or imprecise file identity".into());
    }
    value.as_u64().filter(|number| *number<=9_007_199_254_740_991)
        .ok_or_else(|| "Invalid or imprecise file identity".into())
}

fn expected_identity(value: &Value) -> Result<Option<(u64,u64)>> {
    if value.is_null() { return Ok(None); }
    let object = value.as_object().filter(|fields| fields.len()==2 && fields.contains_key("device") && fields.contains_key("inode"))
        .ok_or("Invalid file identity")?;
    Ok(Some((component(&object["device"])?,component(&object["inode"])?)))
}

fn open_item(path: &Path) -> Result<File> {
    let path = cpath(path)?;
    let fd = unsafe { libc::open(path.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_CLOEXEC|libc::O_NONBLOCK) };
    if fd<0 { return Err(format!("Colour tags require an accessible regular file or folder without following links: {}",std::io::Error::last_os_error())); }
    let file = unsafe { File::from_raw_fd(fd) };
    let mut info: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(file.as_raw_fd(),&mut info) } != 0 { return Err(std::io::Error::last_os_error().to_string()); }
    if info.st_mode & libc::S_IFMT != libc::S_IFREG && info.st_mode & libc::S_IFMT != libc::S_IFDIR {
        return Err("Colour tags can be stored on regular files and folders, without following links".into());
    }
    Ok(file)
}

fn write_tags(request: &Value) -> Result<Value> {
    let ids = validate(&request["tags"])?;
    let file = open_item(path(request,"path")?)?;
    if let Some((dev,ino)) = expected_identity(&request["identity"])? {
        let mut info: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(file.as_raw_fd(),&mut info) } != 0 { return Err(std::io::Error::last_os_error().to_string()); }
        if (info.st_dev,info.st_ino)!=(dev,ino) {
            return Err("This item changed while its tags were being edited; reopen Colour Tags".into());
        }
    }
    let attribute = CString::new(ATTRIBUTE).unwrap();
    if ids.is_empty() {
        if unsafe { libc::fremovexattr(file.as_raw_fd(),attribute.as_ptr()) } != 0 {
            let error = std::io::Error::last_os_error();
            if !error.raw_os_error().is_some_and(missing) {
                return Err(format!("This filesystem or item cannot save colour tags: {error}"));
            }
        }
    } else {
        let raw = json!({"version":1,"tags":ids}).to_string();
        if unsafe { libc::fsetxattr(file.as_raw_fd(),attribute.as_ptr(),raw.as_ptr().cast(),raw.len(),0) } != 0 {
            return Err(format!("This filesystem or item cannot save colour tags: {}",std::io::Error::last_os_error()));
        }
    }
    Ok(projection(read_raw_fd(&file)))
}

fn copy_tags(request: &Value) -> Result<Value> {
    let source = path(request,"source")?;
    let destination = path(request,"destination")?;
    let raw = match read_raw_path(source) {
        Ok(None) => return Ok(json!({"ok":true})),
        Ok(Some(raw)) => raw,
        Err(error) if error.raw_os_error().is_some_and(unsupported) => return Ok(json!({"ok":true})),
        Err(error) if error.raw_os_error()==Some(libc::ERANGE) => return Err("Tag metadata exceeds its size limit".into()),
        Err(error) => return Err(error.to_string()),
    };
    if raw.len()>MAX_RAW { return Err("Tag metadata exceeds its size limit".into()); }
    let destination = cpath(destination)?;
    let attribute = CString::new(ATTRIBUTE).unwrap();
    if unsafe { libc::lsetxattr(destination.as_ptr(),attribute.as_ptr(),raw.as_ptr().cast(),raw.len(),0) } != 0 {
        return Err(format!("Destination cannot preserve colour tags; source data was kept: {}",std::io::Error::last_os_error()));
    }
    Ok(json!({"ok":true}))
}

fn link_error(code: i32) -> (&'static str,&'static str) {
    match code {
        libc::ENOENT | libc::ENOTDIR => ("broken","The symbolic-link target is missing."),
        libc::ELOOP => ("chain-limit","The symbolic-link chain is cyclic or exceeds the operating-system limit."),
        libc::EACCES | libc::EPERM => ("permission-denied","Permission to inspect the symbolic-link target was denied."),
        _ => ("unavailable","The symbolic-link target is unavailable."),
    }
}

fn describe_link(request: &Value) -> Result<Value> {
    let mode = request["mode"].as_u64().filter(|value| *value<=u32::MAX as u64)
        .ok_or("Invalid file mode")? as libc::mode_t;
    if mode & libc::S_IFMT != libc::S_IFLNK {
        return Ok(json!({"link":false,"directory":mode & libc::S_IFMT == libc::S_IFDIR,
            "linkStatus":"","linkError":"","linkDetail":""}));
    }
    let path = cpath(path(request,"path")?)?;
    let mut info: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::stat(path.as_ptr(),&mut info) } == 0 {
        return Ok(json!({"link":true,"directory":info.st_mode & libc::S_IFMT == libc::S_IFDIR,
            "linkStatus":"resolved","linkError":"",
            "linkDetail":"Resolved symbolic link; opening follows its target."}));
    }
    let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    let (state,message) = link_error(code);
    let message = format!("{message} Use File Info to inspect the target or remove the link; this does not remove the target.");
    Ok(json!({"link":true,"directory":false,"linkStatus":state,"linkError":message,"linkDetail":message}))
}

fn identity_details(request: &Value) -> Result<Value> {
    let device = request["device"].as_u64().ok_or("Unsupported native file identity")?;
    let inode = request["inode"].as_u64().ok_or("Unsupported native file identity")?;
    let expected = expected_identity(&request["expected"])?;
    let mode = request["mode"].as_u64().unwrap_or(0);
    let links = request["nlink"].as_u64();
    Ok(json!({"wire":{"device":device.to_string(),"inode":inode.to_string()},
        "renameFollow":mode & libc::S_IFMT as u64 != libc::S_IFDIR as u64 && links==Some(1),
        "matches":expected.is_none_or(|value| value==(device,inode))}))
}

fn inspect(request: &Value) -> Result<Value> {
    let mut result = projection(read_raw_path(path(request,"path")?));
    let link = describe_link(request)?;
    let identity = identity_details(request)?;
    result["linkState"] = link;
    result["identity"] = identity["wire"].clone();
    result["renameFollow"] = identity["renameFollow"].clone();
    Ok(result)
}

fn dispatch(request: &Value) -> Result<Value> {
    match request["action"].as_str() {
        Some("read") => Ok(projection(read_raw_path(path(request,"path")?))),
        Some("write") => write_tags(request),
        Some("copy") => copy_tags(request),
        Some("index") => {
            let result = projection(read_raw_path(path(request,"path")?));
            Ok(json!({"text":result["tags"].as_array().unwrap().iter().map(|tag|
                format!("|{}|",tag["id"].as_str().unwrap())).collect::<String>()}))
        }
        Some("describe-link") => describe_link(request),
        Some("identity") => identity_details(request),
        Some("inspect") => inspect(request),
        _ => Err("Unknown colour tag action".into()),
    }
}

fn main() {
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    let mut stdout = std::io::stdout().lock();
    loop {
        let mut line = Vec::new();
        let mut oversized = false;
        let mut saw_input = false;
        loop {
            let chunk = match stdin.fill_buf() { Ok(chunk) => chunk, Err(_) => return };
            if chunk.is_empty() { if !saw_input { return; } else { break; } }
            saw_input = true;
            let length = chunk.iter().position(|byte| *byte == b'\n').map_or(chunk.len(),|index| index+1);
            let ended = chunk[length-1] == b'\n';
            if line.len()+length>1024*1024 { oversized=true; }
            if !oversized { line.extend_from_slice(&chunk[..length]); }
            stdin.consume(length);
            if ended { break; }
        }
        let result = if oversized { Err("Colour tag request is too large".into()) }
            else { serde_json::from_slice::<Value>(&line).map_err(|error| error.to_string())
                .and_then(|request| dispatch(&request)) };
        let reply = result.unwrap_or_else(|error| json!({"error":error}));
        if writeln!(stdout,"{reply}").and_then(|_|stdout.flush()).is_err() { break; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_rejects_invalid_tags_and_versions() {
        assert_eq!(decode(br#"{"version":1,"tags":["red","blue"]}"#).unwrap(),["red","blue"]);
        for raw in [
            br#"{"version":true,"tags":["red"]}"#.as_slice(),
            br#"{"version":2,"tags":["red"]}"#,
            br#"{"version":1,"tags":["red","red"]}"#,
            br#"{"version":1,"tags":["cyan"]}"#,
            br#"not json"#,
        ] { assert!(decode(raw).is_err()); }
    }

    #[test]
    fn exact_identity_rejects_imprecise_numbers() {
        assert_eq!(component(&json!("18446744073709551615")).unwrap(),u64::MAX);
        assert_eq!(component(&json!(9_007_199_254_740_991_u64)).unwrap(),9_007_199_254_740_991);
        for value in [json!(true),json!(-1),json!(9_007_199_254_740_992_u64),
            json!("01"),json!("18446744073709551616"),json!(1.0)] {
            assert!(component(&value).is_err(),"{value}");
        }
        assert!(expected_identity(&json!({"device":"1","inode":"2","extra":0})).is_err());
    }

    #[test]
    fn index_uses_canonical_order() {
        let raw=br#"{"version":1,"tags":["blue","red"]}"#;
        let tags=projection(Ok(Some(raw.to_vec())));
        assert_eq!(tags["tags"][0]["id"],"red");
        assert_eq!(tags["tags"][1]["id"],"blue");
    }

    #[test]
    fn link_failures_have_stable_user_states() {
        assert_eq!(link_error(libc::ENOENT).0,"broken");
        assert_eq!(link_error(libc::ENOTDIR).0,"broken");
        assert_eq!(link_error(libc::ELOOP).0,"chain-limit");
        assert_eq!(link_error(libc::EACCES).0,"permission-denied");
        assert_eq!(link_error(libc::EPERM).0,"permission-denied");
        assert_eq!(link_error(libc::EIO).0,"unavailable");
    }
}
