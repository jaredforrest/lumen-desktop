use crate::harness::{Fixture, Options, Result, pause, read, require, save, wait_true};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
fn state(f:&Fixture)->Result<Value>{f.state("sessionAudit","last-state.json")}
fn actions(path:&Path)->Vec<String>{fs::read_to_string(path).unwrap_or_default().lines().map(str::to_owned).collect()}
pub fn run(options:Options)->Result<()> {
 let mut f=Fixture::new(options)?;
 let qml=f.qml.clone();
 f.launch(Path::new("quickshell"),&["-p",qml.to_str().unwrap()],"shell.log")?;
 wait_true("session UI available",||Ok(state(&f)?["available"]==true))?;
 let client=f.sibling("session-client")?;
 let output=f.options.output.display().to_string();
 let child=f.launch(&client,&[&output],"client.log")?;
 wait_true("dirty client ready",||Ok(f.options.output.join("client-state.json").exists()))?;
 let marker=f.options.output.join("host-actions");
 f.ipc("sessionAudit","begin",&[])?;
 wait_true("single confirmation visible",||Ok(state(&f)?["visible"]==true))?;
 require(actions(&marker).is_empty(),"Showing confirmation invoked host")?;
 require(read(&f.options.output.join("client-state.json"))?["prompt"]==false,"Showing confirmation requested application quit")?;
 f.call("wtype",&["-s","200","-k","Escape"])?;
 wait_true("Escape cancels",||Ok(state(&f)?["visible"]==false))?;
 require(f.processes[child].alive()? && actions(&marker).is_empty(),"Cancel closed an app or invoked host")?;
 f.ipc("sessionAudit","retireObserver",&[])?;
 wait_true("observer retired",||Ok(state(&f)?["available"]==false))?;
 f.ipc("sessionAudit","showPoweroff",&[])?;
 let visible=state(&f)?;
 require(visible["error"]=="","Shutdown depends on application observation")?;
 require(visible["height"].as_f64().unwrap_or(1000.)<300.,"Confirmation retained excessive empty space")?;
 f.call("grim",&[f.options.output.join("shutdown-confirmation.png").to_str().unwrap()])?;
 f.ipc("sessionAudit","confirm",&[])?;
 wait_true("one click invokes recording poweroff stub",||Ok(actions(&marker)==vec!["poweroff"]))?;
 wait_true("successful action dismisses",||Ok(state(&f)?["visible"]==false))?;
 require(f.processes[child].alive()?,"UI directly killed an application")?;
 f.ipc("sessionAudit","confirm",&[])?;
 pause(200);
 require(actions(&marker)==vec!["poweroff"],"Hidden confirmation repeated action")?;
 fs::write(f.options.output.join("host-actions.error"),"Test logind rejected the request")?;
 f.ipc("sessionAudit","begin",&[])?;
 f.ipc("sessionAudit","confirm",&[])?;
 wait_true("actual command error shown",||Ok(state(&f)?["error"].as_str().is_some_and(|s|s.contains("Test logind rejected the request"))))?;
 require(state(&f)?["visible"]==true,"Failure hid confirmation")?;
 f.call("grim",&[f.options.output.join("shutdown-error.png").to_str().unwrap()])?;
 f.ipc("sessionAudit","cancel",&[])?;
 require(actions(&marker)==vec!["poweroff","reboot"],"Unexpected host actions")?;
 save(&f.options.output.join("result.json"),&json!({"passed":true,"checks":["one normal confirmation; no native quit preflight or checkbox","Escape cancels without touching applications","shutdown works without application observer","compact confirmation geometry","one explicit click invokes recording-only poweroff stub","hidden dialog cannot repeat action","actual system error remains visible"]}))?;
 Ok(())
}
