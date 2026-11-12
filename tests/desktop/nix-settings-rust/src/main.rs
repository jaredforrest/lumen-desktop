//! Exercise the real Rust settings transaction against the actual Nix schema.
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn require(condition: bool, message: &str) -> Result<()> {
    if condition { Ok(()) } else { Err(message.into()) }
}

fn invoke(runtime: &Path, root: &Path, source: &Path, builder: &Path, args: &[&str]) -> Result<Value> {
    let output = Command::new(runtime).args(args)
        .env("XDG_RUNTIME_DIR",root.join("runtime"))
        .env("LUMEN_NIX_SETTINGS_SOURCE",source)
        .env("LUMEN_NIX_SETTINGS_BUILDER",builder)
        .env_remove("LUMEN_NIX_SETTINGS_ACTIVE")
        .env_remove("LUMEN_NIX_SETTINGS_DEPLOY")
        .env_remove("LUMEN_NIX_SETTINGS_BUILD_SOURCE")
        .output()?;
    let value: Value = serde_json::from_slice(&output.stdout)?;
    require(output.status.success(), &format!("Settings command {args:?} failed: {value} / {}",String::from_utf8_lossy(&output.stderr)))?;
    Ok(value)
}

fn reject(runtime: &Path, root: &Path, source: &Path, builder: &Path, value: &str) -> Result<()> {
    let before = fs::read(source)?;
    let output = Command::new(runtime).args(["patch","windows",value])
        .env("XDG_RUNTIME_DIR",root.join("runtime"))
        .env("LUMEN_NIX_SETTINGS_SOURCE",source)
        .env("LUMEN_NIX_SETTINGS_BUILDER",builder)
        .env_remove("LUMEN_NIX_SETTINGS_ACTIVE")
        .env_remove("LUMEN_NIX_SETTINGS_DEPLOY")
        .env_remove("LUMEN_NIX_SETTINGS_BUILD_SOURCE")
        .output()?;
    require(!output.status.success(),"Invalid owned geometry value was accepted")?;
    require(fs::read(source)? == before,"Rejected value changed authoritative source")
}
fn reject_shortcut(runtime:&Path,root:&Path,source:&Path,builder:&Path,expected:&Value,value:&Value)->Result<()> {
    let before=fs::read(source)?;
    let output=Command::new(runtime).args(["compare-set","shortcuts",&expected.to_string(),&value.to_string()])
        .env("XDG_RUNTIME_DIR",root.join("runtime")).env("LUMEN_NIX_SETTINGS_SOURCE",source).env("LUMEN_NIX_SETTINGS_BUILDER",builder)
        .env_remove("LUMEN_NIX_SETTINGS_ACTIVE").env_remove("LUMEN_NIX_SETTINGS_DEPLOY").env_remove("LUMEN_NIX_SETTINGS_BUILD_SOURCE").output()?;
    require(!output.status.success(),"Invalid or stale shortcut edit accepted")?;
    require(fs::read(source)?==before,"Rejected shortcut edit changed authoritative source")
}

fn main() -> Result<()> {
    if env::args().nth(1).as_deref()==Some("queue") {
        fs::write(env::var_os("LUMEN_QUEUE_PROOF").ok_or("No queue proof")?,"queued")?;
        if env::var_os("LUMEN_QUEUE_FAILURE_FILE").is_some_and(|path|Path::new(&path).exists()) {
            println!("{{\"error\":\"fixture worker unavailable\"}}");std::process::exit(1);
        }
        println!("{{}}");
        return Ok(());
    }
    let mut args = env::args().skip(1);
    let runtime = PathBuf::from(args.next().ok_or("Expected Rust settings executable")?);
    let schema = fs::canonicalize(args.next().ok_or("Expected Nix schema")?)?;
    let nixpkgs = fs::canonicalize(args.next().ok_or("Expected nixpkgs path")?)?;
    require(args.next().is_none(),"Unexpected argument")?;
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = env::temp_dir().join(format!("lumen-settings-{}-{unique}",std::process::id()));
    fs::create_dir(&root)?;
    let result = run(&runtime,&schema,&nixpkgs,&root);
    fs::remove_dir_all(&root)?;
    result
}

fn run(runtime: &Path, schema: &Path, nixpkgs: &Path, root: &Path) -> Result<()> {
    let source = root.join("desktop.nix");
    fs::write(&source,"{ }\n")?;
    let builder = root.join("builder.nix");
    fs::write(&builder,format!("{{ source }}: let pkgs = import {} {{ }}; in pkgs.writeTextDir \"settings.json\" (builtins.toJSON (import {} {{ inherit (pkgs) lib; values = import (builtins.toPath source); }}))\n",nixpkgs.display(),schema.display()))?;
    let defaults = invoke(runtime,root,&source,&builder,&["sync"])?;
    require(defaults["windows"]["rememberOwnedGeometry"] == false,"Owned geometry default is not false")?;
    let opted = invoke(runtime,root,&source,&builder,&["patch","windows",r#"{"rememberOwnedGeometry":true}"#])?;
    require(opted["windows"]["rememberOwnedGeometry"] == true,"Opt-in was not set")?;
    let edited = invoke(runtime,root,&source,&builder,&["patch","windows",r#"{"tileGap":12}"#])?;
    require(edited["windows"]["rememberOwnedGeometry"] == true && edited["windows"]["tileGap"] == 12,
        "Unrelated window edit lost owned geometry opt-in")?;
    for invalid in [json!({"rememberOwnedGeometry":1}),json!({"rememberOwnedGeometry":"true"}),json!({"rememberOwnedGeometry":null})] {
        reject(runtime,root,&source,&builder,&invalid.to_string())?;
    }
    let current = root.join("runtime/lumen-nix-settings/current");
    let empty=json!({"rebindings":{}});
    let mapped=json!({"rebindings":{"window.fullscreen.toggle":"Mod4+Control+x"}});
    let changed=invoke(runtime,root,&source,&builder,&["compare-set","shortcuts",&empty.to_string(),&mapped.to_string()])?;
    require(changed["shortcuts"]==mapped,"Typed shortcut transaction failed")?;
    reject_shortcut(runtime,root,&source,&builder,&empty,&json!({"rebindings":{"window.fullscreen.toggle":null}}))?;
    for invalid in [json!({"rebindings":{"window.fullscreen.toggle":true}}),json!({"rebindings":{"window.fullscreen.toggle":"--release Super_L"}}),json!({"rebindings":{"window.fullscreen.toggle":"Control+x exec invalid"}})] {
        reject_shortcut(runtime,root,&source,&builder,&mapped,&invalid)?;
    }
    require(current.is_symlink(),"Runtime bundle symlink missing")?;
    fs::remove_file(&current)?;
    let restored = invoke(runtime,root,&source,&builder,&["sync"])?;
    require(restored["windows"]["rememberOwnedGeometry"] == true && restored["windows"]["tileGap"] == 12,
        "Authoritative settings did not survive runtime bundle loss")?;
    require(current.is_symlink(),"Runtime bundle was not restored")?;
    require(restored["shortcuts"]==mapped,"Shortcut preference did not survive runtime bundle loss")?;
    let disabled=json!({"rebindings":{"window.fullscreen.toggle":null}});
    let value=invoke(runtime,root,&source,&builder,&["compare-set","shortcuts",&mapped.to_string(),&disabled.to_string()])?;
    require(value["shortcuts"]==disabled,"Explicit shortcut disable was not durable")?;
    let value=invoke(runtime,root,&source,&builder,&["compare-set","shortcuts",&disabled.to_string(),&empty.to_string()])?;
    require(value["shortcuts"]==empty && value["windows"]["tileGap"]==12,"Restore lost unrelated settings")?;
    asynchronous_contract(runtime,root,&source,&builder)?;
    println!("Rust settings transaction retained owned geometry opt-in through unrelated edit and runtime loss");
    Ok(())
}

fn asynchronous_contract(runtime:&Path,root:&Path,initial:&Path,builder:&Path)->Result<()> {
    let repo=root.join("repo");fs::create_dir_all(repo.join("settings"))?;
    let source=repo.join("settings/desktop.nix");fs::copy(initial,&source)?;
    for args in [vec!["init","-b","main"],vec!["config","user.name","Fixture"],vec!["config","user.email","fixture@example.invalid"],vec!["add","."],vec!["commit","-m","baseline"]] {
        require(Command::new("git").arg("-C").arg(&repo).args(args).output()?.status.success(),"Fixture Git initialization failed")?;
    }
    let active=root.join("installed.json");
    let baseline=invoke(runtime,root,initial,builder,&["read"])?;
    fs::write(&active,baseline.to_string())?;
    let call=|args:&[&str]|->Result<std::process::Output>{Ok(Command::new(runtime).args(args)
        .env("XDG_RUNTIME_DIR",root.join("async-runtime")).env("XDG_STATE_HOME",root.join("async-state"))
        .env("LUMEN_NIX_SETTINGS_SOURCE",&source).env("LUMEN_NIX_SETTINGS_BUILDER",builder)
        .env("LUMEN_NIX_SETTINGS_ACTIVE",&active).env("LUMEN_NIX_SETTINGS_DEPLOY",env::current_exe()?)
        .env("LUMEN_NIX_SETTINGS_BUILD_SOURCE",&repo).env("LUMEN_NIX_SETTINGS_ASYNC","1")
        .env("LUMEN_QUEUE_PROOF",root.join("queued")).env("LUMEN_QUEUE_FAILURE_FILE",root.join("queue-failure")).output()?)};
    let start=std::time::Instant::now();
    let saved=call(&["patch","appearance",r#"{"accent":"green"}"#])?;
    require(saved.status.success(),&format!("Async save failed: {}",String::from_utf8_lossy(&saved.stdout)))?;
    println!("First validated asynchronous save returned in {:?}",start.elapsed());
    require(root.join("queued").exists(),"Save did not queue propagation")?;
    let read:Value=serde_json::from_slice(&call(&["read"])?.stdout)?;
    require(read["appearance"]["accent"]=="green","A new reader lost the saved preference before deployment")?;
    let second=call(&["patch","appearance",r#"{"mode":"dark"}"#])?;
    require(second.status.success(),"Second pending save failed")?;
    let read:Value=serde_json::from_slice(&call(&["read"])?.stdout)?;
    require(read["appearance"]["accent"]=="green" && read["appearance"]["mode"]=="dark","Queued save lost an earlier unapplied preference")?;
    let before=fs::read(&source)?;
    require(!call(&["patch","appearance",r#"{"accent":"invalid"}"#])?.status.success(),"Invalid async save accepted")?;
    require(fs::read(&source)?==before,"Invalid async save changed source")?;
    fs::write(root.join("queue-failure"),"fail")?;
    require(call(&["patch","appearance",r#"{"accent":"orange"}"#])?.status.success(),"Worker startup failure lost a committed save")?;
    let read:Value=serde_json::from_slice(&call(&["read"])?.stdout)?;
    require(read["appearance"]["accent"]=="orange","Worker failure reverted the saved preference")?;
    // Cache is a projection of exact source bytes, never a competing authority.
    fs::write(&source,"{}\n")?;
    let read:Value=serde_json::from_slice(&call(&["read"])?.stdout)?;
    require(read==baseline,"Source replacement left a stale settings projection active")?;
    println!("PASS asynchronous saves queue without rebuilding, remain readable across processes and retain successive edits ({:?})",start.elapsed());
    Ok(())
}
