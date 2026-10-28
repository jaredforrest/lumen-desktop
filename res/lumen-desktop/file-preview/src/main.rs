//! Bounded local Finder and Spotlight previews.
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::env;
use std::fs::{self,File,OpenOptions};
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt,OpenOptionsExt,PermissionsExt};
use std::path::{Path,PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T,String>;
fn uri(path:&Path)->String {
    let mut value="file://".to_owned();
    for byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte){value.push(*byte as char);}
        else{value.push_str(&format!("%{byte:02X}"));}
    }
    value
}
fn limited(args:&[&str])->Result<Vec<u8>> {
    let output=Command::new("timeout").args(["--signal=KILL","8s","prlimit","--as=805306368","--cpu=6","--fsize=16777216","--"])
        .args(args).output().map_err(|e|e.to_string())?;
    if !output.status.success(){return Err(if matches!(output.status.code(),Some(124|137)) { "Preview helper timed out" } else { "Preview helper failed" }.into());}
    Ok(output.stdout)
}
fn cache_directory()->Result<PathBuf> {
    let base=env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(||PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".cache"));
    let cache=base.join("lumen-desktop/previews");fs::create_dir_all(&cache).map_err(|e|e.to_string())?;
    fs::set_permissions(&cache,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
    Ok(cache)
}
fn pdf(path:&Path,metadata:&fs::Metadata,page:u32,resolution:u32,result:&mut Value)->Result<()> {
    let details=String::from_utf8_lossy(&limited(&["pdfinfo",path.to_str().ok_or("Invalid path")?])?).into_owned();
    let pages=details.lines().find_map(|line|line.strip_prefix("Pages:").and_then(|value|value.trim().parse::<u32>().ok())).unwrap_or(1).max(1);
    let page=page.min(pages);
    let key=format!("{}:{}:{}:{}:{}:{}:{}",path.display(),metadata.dev(),metadata.ino(),metadata.len(),metadata.mtime()*1_000_000_000+metadata.mtime_nsec(),page,resolution);
    let digest=Sha256::digest(key.as_bytes());
    let target=cache_directory()?.join(format!("{digest:x}.png"));
    if !target.exists() {
        let directory=target.parent().ok_or("Invalid preview cache")?;
        let temp=tempfile::Builder::new().prefix("preview-").tempdir_in(directory).map_err(|e|e.to_string())?;
        let prefix=temp.path().join("page");
        limited(&["pdftoppm","-f",&page.to_string(),"-singlefile","-scale-to",&resolution.to_string(),"-png",
            path.to_str().ok_or("Invalid path")?,prefix.to_str().ok_or("Invalid cache path")?])?;
        let rendered=prefix.with_extension("png");
        fs::set_permissions(&rendered,fs::Permissions::from_mode(0o600)).map_err(|e|e.to_string())?;
        fs::rename(rendered,&target).map_err(|e|e.to_string())?;
        let mut files=fs::read_dir(directory).map_err(|e|e.to_string())?.filter_map(|entry|entry.ok().map(|entry|entry.path()))
            .filter(|path|path.extension().is_some_and(|ext|ext=="png"))
            .filter_map(|path|fs::metadata(&path).ok().and_then(|meta|meta.modified().ok()).map(|time|(time,path)))
            .collect::<Vec<_>>();
        files.sort_by(|a,b|b.0.cmp(&a.0));
        for (_,old) in files.into_iter().skip(100){let _=fs::remove_file(old);}
    }
    result["type"]=json!("image");result["uri"]=json!(uri(&target));
    result["text"]=json!(format!("Page {page} of {pages}"));
    result["page"]=json!(page);result["pages"]=json!(pages);result["truncated"]=json!(false);
    Ok(())
}
fn media(path:&Path,result:&mut Value)->Result<()> {
    let bytes=limited(&["ffprobe","-v","error","-show_entries","format=duration:stream=codec_type,codec_name,width,height,sample_rate,channels","-of","json",path.to_str().ok_or("Invalid path")?])?;
    let data:Value=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;
    let seconds=data["format"]["duration"].as_str().and_then(|value|value.parse::<f64>().ok())
        .or_else(||data["format"]["duration"].as_f64()).unwrap_or(0.);
    if !seconds.is_finite() || seconds<0. {return Err("Invalid media duration".into());}
    let whole=seconds as u64;let mut lines=vec![format!("Duration: {}:{:02}",whole/60,whole%60)];
    for stream in data["streams"].as_array().into_iter().flatten().take(8) {
        let kind=stream["codec_type"].as_str().unwrap_or("");
        let label=format!("{}: {}",kind.chars().take(1).collect::<String>().to_uppercase()+&kind.chars().skip(1).collect::<String>(),stream["codec_name"].as_str().unwrap_or(""));
        let mut label=label;
        if let (Some(width),Some(height))=(stream["width"].as_u64(),stream["height"].as_u64()){label.push_str(&format!(" · {width} × {height}"));}
        if let Some(channels)=stream["channels"].as_u64(){label.push_str(&format!(" · {channels} channels"));}
        lines.push(label);
    }
    result["type"]=json!("media");result["uri"]=json!(uri(path));result["duration"]=json!(seconds);result["text"]=json!(lines.join("\n"));
    Ok(())
}
fn document(path:&Path,result:&mut Value)->Result<()> {
    let file=File::open(path).map_err(|e|e.to_string())?;
    let mut archive=zip::ZipArchive::new(file).map_err(|e|e.to_string())?;
    let entry=if path.extension().is_some_and(|ext|ext.eq_ignore_ascii_case("docx")){"word/document.xml"}else{"content.xml"};
    let member=archive.by_name(entry).map_err(|e|e.to_string())?;
    if member.size()>4*1024*1024 {result["text"]=json!("Document is too large for a quick preview");return Ok(());}
    let mut xml=String::new();member.take(4*1024*1024+1).read_to_string(&mut xml).map_err(|e|e.to_string())?;
    if xml.len()>4*1024*1024{return Err("Document too large".into());}
    let tree=roxmltree::Document::parse(&xml).map_err(|e|e.to_string())?;
    let paragraphs=tree.descendants().filter(|node|node.is_element() && ["p","h"].contains(&node.tag_name().name()))
        .map(|node|node.descendants().filter_map(|child|if child.is_text(){child.text()}else{None}).collect::<String>())
        .collect::<Vec<_>>();
    let text=paragraphs.join("\n");let truncated=text.chars().count()>8192;
    result["type"]=json!("text");result["text"]=json!(if text.is_empty(){"No document text".into()}else{text.chars().take(8192).collect::<String>()});
    result["truncated"]=json!(truncated);Ok(())
}
fn text_preview(path:&Path,result:&mut Value)->Result<()> {
    let stream=OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(path).map_err(|e|e.to_string())?;
    if !stream.metadata().map_err(|e|e.to_string())?.is_file(){return Ok(());}
    let mut raw=Vec::new();stream.take(8193).read_to_end(&mut raw).map_err(|e|e.to_string())?;
    if raw.contains(&0){return Ok(());}
    let source=&raw[..raw.len().min(8192)];
    let text=match std::str::from_utf8(source) {
        Ok(text)=>text,
        Err(error) if raw.len()>8192 && error.error_len().is_none()=>std::str::from_utf8(&source[..error.valid_up_to()]).map_err(|e|e.to_string())?,
        Err(_)=>return Ok(()),
    };
    if text.chars().any(|character|character<' ' && !"\n\r\t".contains(character)){return Ok(());}
    let lines=text.lines().collect::<Vec<_>>();
    result["type"]=json!("text");result["text"]=json!(lines.iter().take(80).copied().collect::<Vec<_>>().join("\n"));
    result["truncated"]=json!(raw.len()>8192 || lines.len()>80);Ok(())
}
fn preview(value:&str,page:u32,resolution:u32)->Result<Value> {
    let page=page.clamp(1,100000);let resolution=resolution.clamp(600,2400);
    let path=Path::new(value);let path=if path.is_absolute(){path.to_path_buf()}else{env::current_dir().map_err(|e|e.to_string())?.join(path)};
    let metadata=fs::metadata(&path).map_err(|e|e.to_string())?;
    let name=path.file_name().ok_or("Invalid preview path")?.to_string_lossy().into_owned();
    let mut result=json!({"path":path,"name":name,"title":name,"size":metadata.len(),
        "mtime":metadata.mtime() as f64+metadata.mtime_nsec() as f64/1e9,
        "type":"generic","text":"This format has no supported preview","reason":"unsupported"});
    if metadata.is_dir() {
        let mut children=Vec::new();
        for entry in fs::read_dir(&path).map_err(|e|e.to_string())? {
            let entry=entry.map_err(|e|e.to_string())?;let filename=entry.file_name().to_string_lossy().into_owned();
            if !filename.starts_with('.') {children.push(format!("{}{}",filename,if entry.file_type().is_ok_and(|kind|kind.is_dir()){ "/" }else{""}));}
            if children.len()>=21{break;}
        }
        let truncated=children.len()>20;children.truncate(20);children.sort_by_key(|name|name.to_lowercase());
        result["type"]=json!("folder");result["reason"]=Value::Null;result["text"]=json!(if children.is_empty(){"Empty folder".into()}else{children.join("\n")});
        result["truncated"]=json!(truncated);return Ok(result);
    }
    if !metadata.is_file(){return Ok(result);}
    let mime=mime_guess::from_path(&path).first_raw().unwrap_or("");result["mime"]=json!(mime);
    if mime.starts_with("image/") && metadata.len()>32*1024*1024 {
        result["reason"]=json!("policy_limit");result["text"]=json!("Inline image previews support files up to 32 MiB");return Ok(result);
    }
    if ["image/png","image/jpeg","image/gif","image/webp","image/bmp"].contains(&mime) && metadata.len()<=32*1024*1024 {
        result["type"]=json!("image");result["reason"]=Value::Null;result["uri"]=json!(uri(&path));result["text"]=json!("");return Ok(result);
    }
    if mime=="application/pdf" && metadata.len()<=64*1024*1024 {
        if let Err(error)=pdf(&path,&metadata,page,resolution,&mut result){
            result["reason"]=json!(if error=="Preview helper timed out" {"provider_timeout"} else {"provider_failure"});
            result["text"]=json!(if error=="Preview helper timed out" {"PDF preview timed out"} else {"PDF preview unavailable (encrypted, damaged, too complex, or renderer unavailable)"});
        }
        else {result["reason"]=Value::Null;}
        return Ok(result);
    }
    if mime.starts_with("audio/") || mime.starts_with("video/") {
        if let Err(error)=media(&path,&mut result){result["reason"]=json!(if error=="Preview helper timed out" {"provider_timeout"} else {"provider_failure"});result["text"]=json!(if error=="Preview helper timed out" {"Media preview timed out"} else {"Media information unavailable"});}
        else {result["reason"]=Value::Null;}
        return Ok(result);
    }
    if path.extension().is_some_and(|ext|ext.eq_ignore_ascii_case("docx") || ext.eq_ignore_ascii_case("odt")) && metadata.len()<=64*1024*1024 {
        if document(&path,&mut result).is_err(){result["reason"]=json!("provider_failure");result["text"]=json!("Document preview unavailable");}
        else if result["type"]=="text" {result["reason"]=Value::Null;} else {result["reason"]=json!("policy_limit");}
        return Ok(result);
    }
    text_preview(&path,&mut result)?;
    if result["type"]=="text" {result["reason"]=Value::Null;}
    Ok(result)
}
fn main() {
    let args=env::args().skip(1).collect::<Vec<_>>();
    let result=args.first().ok_or_else(||"Missing preview path".to_owned()).and_then(|path|{
        let page=args.get(1).map(|value|value.parse::<u32>().map_err(|e|e.to_string())).transpose()?.unwrap_or(1);
        let resolution=args.get(2).map(|value|value.parse::<u32>().map_err(|e|e.to_string())).transpose()?.unwrap_or(1400);
        preview(path,page,resolution)
    });
    match result {Ok(value)=>println!("{value}"),Err(error)=>println!("{}",json!({"error":error}))}
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn text_folder_and_document_previews() {
        let root=tempfile::tempdir().unwrap();let file=root.path().join("example.txt");
        fs::write(&file,"Plain text\n<b>literal</b>").unwrap();
        let text=preview(file.to_str().unwrap(),1,1400).unwrap();
        assert_eq!(text["type"],"text");assert_eq!(text["text"],"Plain text\n<b>literal</b>");
        assert_eq!(preview(root.path().to_str().unwrap(),1,1400).unwrap()["type"],"folder");
        let document=root.path().join("document.docx");
        let file=File::create(&document).unwrap();let mut archive=zip::ZipWriter::new(file);
        use std::io::Write;
        archive.start_file("word/document.xml",zip::write::SimpleFileOptions::default()).unwrap();
        archive.write_all(b"<document><p><t>Office text</t></p></document>").unwrap();archive.finish().unwrap();
        assert_eq!(preview(document.to_str().unwrap(),1,1400).unwrap()["text"],"Office text");
    }
    #[test] fn binary_and_invalid_utf8_stay_generic() {
        let root=tempfile::tempdir().unwrap();let file=root.path().join("sample.bin");
        fs::write(&file,b"hello\0world").unwrap();
        assert_eq!(preview(file.to_str().unwrap(),1,1400).unwrap()["type"],"generic");
    }
    #[test] fn bounded_helper_expires_without_blocking_preview() {
        let started=std::time::Instant::now();
        assert!(limited(&["sh","-c","exec sleep 60"]).is_err());
        assert!(started.elapsed().as_secs()<=10);
    }
}
