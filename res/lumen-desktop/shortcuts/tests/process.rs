//! Shortcut collisions are physical and scope aware; hints require valid authority.
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn entry(id:&str,chord:&str)->Value{json!({"id":id,"action":id,"chord":chord})}
fn run(document:Value,output_file:bool)->(i32,Value,String){
    let nonce=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root=std::env::temp_dir().join(format!("lumen-shortcuts-{}-{nonce}",std::process::id()));
    fs::create_dir(&root).unwrap();
    let source=root.join("input.json");fs::write(&source,document.to_string()).unwrap();
    let output=root.join("output.json");
    let mut command=Command::new(env!("CARGO_BIN_EXE_lumen-shortcuts"));
    command.arg(&source);
    if output_file{command.args(["--output",output.to_str().unwrap()]);}
    let result=command.output().unwrap();
    let bytes=if output_file{fs::read(&output).unwrap()}else{result.stdout};
    let value:Value=serde_json::from_slice(&bytes).unwrap();
    let stderr=String::from_utf8(result.stderr).unwrap();
    fs::remove_dir_all(root).unwrap();
    (result.status.code().unwrap(),value,stderr)
}
fn validate(rows:Vec<Value>)->Value{
    let (code,value,stderr)=run(json!({"version":1,"bindings":rows}),false);
    assert_ne!(code,2,"{stderr}: {value}");value
}
fn invalid(row:Value){
    let (code,value,_)=run(json!({"version":1,"bindings":[row]}),false);
    assert_eq!(code,2,"{value}");
    assert_eq!(value["diagnostics"][0]["code"],"invalid_registry");
}

#[test]
fn aliases_order_and_named_keys_collide(){
    let value=validate(vec![entry("first","Mod4+Control+Shift+f"),entry("other","Shift+Ctrl+Super+F")]);
    assert_eq!(value["valid"],false);
    assert_eq!(value["diagnostics"][0]["bindings"],json!(["first","other"]));
    for (first,second) in [("Control+Esc","Ctrl+Escape"),("Ctrl+F1","Control+f1")] {
        assert_eq!(validate(vec![entry("a",first),entry("b",second)])["valid"],false);
    }
    assert_eq!(validate(vec![entry("fill","Super+Ctrl+Shift+f"),entry("fullscreen","Ctrl+Super+f")])["valid"],true);
}

#[test]
fn trigger_mode_scope_and_devices_are_explicit(){
    let mut mode=entry("other-mode","Super_L");mode["mode"]=json!("resize");
    assert_eq!(validate(vec![entry("press","Super_L"),entry("release","--release Super_L"),mode])["valid"],true);
    let mut editor=entry("editor","Ctrl+s");editor["scope"]=json!("application");editor["appId"]=json!("test.editor");
    let mut writer=entry("writer","Control+s");writer["scope"]=json!("application");writer["appId"]=json!("test.writer");
    assert_eq!(validate(vec![editor.clone(),writer])["valid"],true);
    assert_eq!(validate(vec![entry("global","Ctrl+s"),editor.clone()])["valid"],false);
    let mut current=entry("current","Ctrl+s");current["scope"]=json!("current-application");
    assert_eq!(validate(vec![current,editor])["valid"],false);
    let mut a=entry("a","Ctrl+s");a["device"]=json!("keyboard-a");
    let mut b=entry("b","Ctrl+s");b["device"]=json!("keyboard-b");
    assert_eq!(validate(vec![a,b])["valid"],true);
    assert_eq!(validate(vec![entry("a","Ctrl+s"),entry("b","--input-device=keyboard-b Ctrl+s")])["valid"],false);
    assert_eq!(validate(vec![entry("lock","--locked Super+q"),entry("other","Mod4+q")])["valid"],false);
}

#[test]
fn duplicate_actions_and_disabled_bindings(){
    let mut a=entry("a","Super+Tab");a["action"]=json!("switch");
    let mut b=entry("b","Mod4+Tab");b["action"]=json!("switch");
    assert_eq!(validate(vec![a,b])["valid"],false);
    let mut disabled=entry("b","Control+x");disabled["enabled"]=json!(false);
    assert_eq!(validate(vec![entry("a","Ctrl+x"),disabled])["valid"],true);
}

#[test]
fn malformed_registry_and_ambiguous_bindings_rejected(){
    for row in [entry("a","Ctrl+Control+x"),entry("a","AltGr+x"),entry("a","--made-up Ctrl+x")] {invalid(row);}
    let mut missing_app=entry("a","Ctrl+x");missing_app["scope"]=json!("application");invalid(missing_app);
}

#[test]
fn all_typed_invalid_cases_rejected(){
    let mut rows=Vec::new();
    let mut wildcard=entry("a","Ctrl+x");wildcard["scope"]=json!("application");wildcard["appId"]=json!("test.*");rows.push(wildcard);
    let mut global=entry("a","Ctrl+x");global["scope"]=json!("global");global["appId"]=json!("test.editor");rows.push(global);
    let mut enabled=entry("a","Ctrl+x");enabled["enabled"]=json!(1);rows.push(enabled);
    let mut trigger=entry("a","--release Ctrl+x");trigger["trigger"]=json!("press");rows.push(trigger);
    let mut unknown=entry("a","Ctrl+x");unknown["unknown"]=json!(true);rows.push(unknown);
    for row in rows{invalid(row);}
    let (code,_,_)=run(json!({"version":true,"bindings":[]}),false);assert_eq!(code,2);
    let (code,_,_)=run(json!({"version":1,"bindings":[entry("same","Ctrl+x"),entry("same","Ctrl+y")]}),false);assert_eq!(code,2);
}

#[test]
fn checked_default_registry_and_reachable_hints(){
    let path=std::env::var_os("LUMEN_SHORTCUT_DEFAULTS").map(PathBuf::from)
        .unwrap_or_else(||PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime/shortcut-defaults.json"));
    let source:Value=serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let (_,default,_)=run(source,false);
    assert_eq!(default["valid"],true,"{}",default["diagnostics"]);
    assert!(default["bindings"].as_array().unwrap().iter().any(|row|row["action"]=="window.fill"));
    let mut rows=Vec::new();
    for (id,chord,action) in [("a","Super+x","open"),("b","Ctrl+y","open"),("release","--release Super_L","release"),
        ("mode","Ctrl+z","mode"),("device","Ctrl+w","device"),("disabled","Ctrl+a","disabled"),
        ("reserved","Ctrl+b","reserved"),("unknown","Ctrl+c","consumer.binding")] {
        let mut row=entry(id,chord);row["action"]=json!(action);
        match id {"mode"=>row["mode"]=json!("resize"),"device"=>row["device"]=json!("test"),
            "disabled"=>row["enabled"]=json!(false),"reserved"=>row["owner"]=json!("reserved"),_=>{}}
        rows.push(row);
    }
    let value=validate(rows);
    assert_eq!(value["valid"],true);
    assert_eq!(value["hints"].as_array().unwrap().len(),1);
    assert_eq!(value["hints"][0]["action"],"open");
    let mut ids:Vec<_>=value["hints"][0]["bindings"].as_array().unwrap().iter().map(|row|row["id"].as_str().unwrap()).collect();
    ids.sort_unstable();
    assert_eq!(ids,["a","b"]);
    assert_eq!(validate(vec![entry("a","Ctrl+s"),entry("b","Control+s")])["hints"],json!([]));
}

#[test]
fn collisions_fail_cli_with_diagnostics(){
    let (code,value,stderr)=run(json!({"version":1,"bindings":[entry("a","Super+x"),entry("b","Mod4+x")]}),true);
    assert_eq!(code,1);assert_eq!(value["diagnostics"][0]["code"],"shortcut_collision");
    assert!(stderr.contains("a, b"));
}
