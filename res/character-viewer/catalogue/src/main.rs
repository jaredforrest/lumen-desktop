use character_viewer_catalogue::{catalogue,load_state,render_sections,save_state};
use serde_json::{Value,json};
use std::fs;
use std::io::{self,BufRead,Read,Write};
use std::path::Path;

fn serve(path: &Path) -> io::Result<()> {
    let data=catalogue(&fs::read_to_string(path)?);
    let mut output=io::stdout().lock();
    writeln!(output,"{data}")?;output.flush()?;
    for line in io::stdin().lock().lines() {
        let line=line?;
        let result=if line.len()>1024*1024 { json!({"error":"Character viewer request is too large"}) }
            else if let Ok(request)=serde_json::from_str::<Value>(&line) {
                if request["action"]=="render" {
                    render_sections(&data,&request["state"],request["section"].as_str().unwrap_or("Recent"),
                        request["query"].as_str().unwrap_or(""),request["expanded"].as_bool().unwrap_or(false))
                } else { json!({"error":"Unknown character viewer action"}) }
            } else { json!({"error":"Invalid character viewer request"}) };
        writeln!(output,"{result}")?;output.flush()?;
    }
    Ok(())
}

fn main() {
    let mut args=std::env::args_os().skip(1);
    let Some(action)=args.next() else { eprintln!("Emoji catalogue path is required");std::process::exit(2); };
    if action == "--serve" {
        let Some(path)=args.next() else { std::process::exit(2) };
        if let Err(error)=serve(Path::new(&path)) {eprintln!("{error}");std::process::exit(1)}
        return;
    }
    let result = if action == "--load-state" {
        let Some(path)=args.next() else { std::process::exit(2) };
        Ok(load_state(Path::new(&path)))
    } else if action == "--save-state" {
        let Some(path)=args.next() else { std::process::exit(2) };
        let mut raw=String::new();
        io::stdin().take(1024*1024+1).read_to_string(&mut raw).unwrap_or_else(|error| {eprintln!("{error}");std::process::exit(1)});
        if raw.len()>1024*1024 { eprintln!("Character viewer state is too large");std::process::exit(1); }
        match serde_json::from_str::<Value>(&raw) {
            Ok(value)=>save_state(Path::new(&path),&value).map(|_|json!({"ok":true})),
            Err(error)=>Err(io::Error::other(error)),
        }
    } else {
        fs::read_to_string(&action).map(|source|catalogue(&source))
    };
    let value=result.unwrap_or_else(|error| {eprintln!("{error}");std::process::exit(1)});
    if writeln!(io::stdout().lock(),"{value}").is_err() { std::process::exit(1); }
}
