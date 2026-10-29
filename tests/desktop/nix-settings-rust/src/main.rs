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

fn main() -> Result<()> {
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
    require(current.is_symlink(),"Runtime bundle symlink missing")?;
    fs::remove_file(&current)?;
    let restored = invoke(runtime,root,&source,&builder,&["sync"])?;
    require(restored["windows"]["rememberOwnedGeometry"] == true && restored["windows"]["tileGap"] == 12,
        "Authoritative settings did not survive runtime bundle loss")?;
    require(current.is_symlink(),"Runtime bundle was not restored")?;
    println!("Rust settings transaction retained owned geometry opt-in through unrelated edit and runtime loss");
    Ok(())
}
