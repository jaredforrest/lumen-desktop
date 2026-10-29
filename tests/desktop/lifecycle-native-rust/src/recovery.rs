//! Shell SIGKILL leaves two dirty GTK documents alive and restores useful controls.
use crate::harness::{Fixture, Options, Result, read, require, save, wait_true};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::time::Instant;

fn probe(f: &Fixture) -> Result<Value> { f.state("recoveryProbe", "last-recovery.json") }
fn document(f: &Fixture, name: &str) -> Result<Value> { read(&f.options.output.join(format!("client-{name}.json"))) }
fn windows(f: &Fixture) -> Result<Vec<Value>> {
    let mut all = Vec::new();
    Fixture::nodes(&f.tree()?, &mut all);
    Ok(all.into_iter().filter(|node| node["app_id"].as_str().is_some_and(|id| id.starts_with("org.lumen.Recovery"))).collect())
}
fn window(f: &Fixture, name: &str) -> Result<Value> {
    windows(f)?.into_iter().find(|node| node["app_id"] == format!("org.lumen.Recovery{name}"))
        .ok_or_else(|| format!("Missing recovery window {name}").into())
}
fn identity(pid: u32) -> Result<Value> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let fields: Vec<&str> = stat.rsplit_once(')').ok_or("Bad proc stat")?.1.split_whitespace().collect();
    Ok(json!({"pid":pid,"startTimeTicks":fields.get(19).ok_or("Missing start time")?.parse::<u64>()?,
        "executable":fs::read_link(format!("/proc/{pid}/exe"))?}))
}
fn focus(f: &Fixture, name: &str) -> Result<()> {
    f.call("swaymsg", &[&format!("[app_id=\"org.lumen.Recovery{name}\"]"), "focus"])?;
    wait_true(&format!("native client focus {name}"), || Ok(document(f,name)?["focused"] == true))
}
fn text(f: &Fixture, value: &str) -> Result<()> {
    f.call("wtype", &["-s", "50", value])?;
    Ok(())
}
fn ready(f: &Fixture) -> Result<bool> {
    let value = probe(f)?;
    Ok(value["bars"] == json!(["HEADLESS-1"]) && value["dockItems"].as_u64().is_some_and(|count| count > 0))
}
fn menu(f: &Fixture) -> Result<()> {
    f.ipc("desktop", "menu", &["Desktop"])?;
    wait_true("captured Desktop menu", || {
        let value: Value = serde_json::from_str(&f.ipc("desktop", "layout", &[])?)?;
        Ok(value.as_array().is_some_and(|items| items.iter().any(|item| item["menu"] == "Desktop")))
    })
}
fn dock(f: &Fixture) -> Result<()> {
    f.ipc("desktop", "focusDock", &[])?;
    wait_true("dock keyboard capture", || Ok(probe(f)?["popupOwner"].as_str().is_some_and(|value| value.starts_with("dock:"))))
}
fn release(f: &Fixture) -> Result<()> {
    f.call("wtype", &["-k", "Escape"])?;
    wait_true("keyboard capture released", || Ok(probe(f)?["popupOwner"] == ""))
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    f.launch(Path::new("wtype"), &["-s", "60000"], "keyboard.log")?;
    let client = f.sibling("recovery-client")?;
    let output = f.options.output.display().to_string();
    let mut clients = Vec::new();
    for name in ["A", "B"] { clients.push((name, f.launch(&client, &[&output,name], &format!("client-{name}.log"))?)); }
    wait_true("two native GTK documents", || Ok(windows(&f)?.len() == 2))?;
    let mut expected = ["Synthetic unsaved document A".to_owned(), "Synthetic unsaved document B".to_owned()];
    for (index,name) in ["A", "B"].iter().enumerate() {
        focus(&f,name)?;
        text(&f,&expected[index])?;
        wait_true(&format!("dirty document {name}"), || Ok(document(&f,name)?["text"] == expected[index] && document(&f,name)?["modified"] == true))?;
    }
    let before = clients.iter().map(|(name,index)| -> Result<Value> {
        let pid = f.processes[*index].0.id();
        Ok(json!({"name":name,"identity":identity(pid)?,"container":window(&f,name)?["id"],"document":document(&f,name)?}))
    }).collect::<Result<Vec<_>>>()?;
    let qml = f.qml.clone();
    let mut shell = f.launch(Path::new("quickshell"), &["-p",qml.to_str().ok_or("QML path UTF-8")?], "shell-0.log")?;
    wait_true("initial useful shell", || ready(&f))?;
    let mut receipts = Vec::new();
    for (cycle,held) in ["menu", "dock"].iter().enumerate() {
        focus(&f,"A")?;
        if *held == "menu" { menu(&f)?; } else { dock(&f)?; }
        let old_pid = f.processes[shell].0.id();
        require(f.processes[shell].alive()?, "Shell died before requested crash")?;
        unsafe { libc::kill(old_pid as i32, libc::SIGKILL); }
        let dead = f.processes[shell].0.wait()?;
        require(dead.signal() == Some(libc::SIGKILL), "Shell did not die by SIGKILL")?;
        let suffix = format!(" gap{}",cycle+1);
        expected[0].push_str(&suffix);
        text(&f,&suffix)?;
        wait_true("typing after shell death", || Ok(document(&f,"A")?["text"] == expected[0]))?;
        let start = Instant::now();
        shell = f.launch(Path::new("quickshell"), &["-p",qml.to_str().ok_or("QML path UTF-8")?], &format!("shell-{}.log",cycle+1))?;
        wait_true("restarted useful shell", || ready(&f))?;
        menu(&f)?;
        let elapsed = start.elapsed().as_secs_f64();
        require(elapsed <= 5.0, format!("Recovered menu took {elapsed:.2}s"))?;
        release(&f)?;
        focus(&f,"B")?;
        let restored = format!(" restored{}",cycle+1);
        expected[1].push_str(&restored);
        text(&f,&restored)?;
        wait_true("typing after recovered menu", || Ok(document(&f,"B")?["text"] == expected[1]))?;
        dock(&f)?;
        release(&f)?;
        let dock_text = format!(" dock{}",cycle+1);
        expected[1].push_str(&dock_text);
        text(&f,&dock_text)?;
        wait_true("typing after recovered dock", || Ok(document(&f,"B")?["text"] == expected[1]))?;
        let current = windows(&f)?;
        require(current.len() == 2, "Recovery duplicated or lost client windows")?;
        for (index,(name,client_index)) in clients.iter().enumerate() {
            require(f.processes[*client_index].alive()?, format!("Client {name} exited"))?;
            require(identity(f.processes[*client_index].0.id())? == before[index]["identity"], "Client process identity changed")?;
            require(window(&f,name)?["id"] == before[index]["container"], "Compositor container identity changed")?;
            let status = document(&f,name)?;
            require(status["modified"] == true && status["text"] == expected[index], format!("Dirty document {name} changed"))?;
        }
        let tracked = probe(&f)?["windows"].as_array().cloned().unwrap_or_default().into_iter()
            .filter(|item| item["app"].as_str().is_some_and(|value| value.starts_with("org.lumen.Recovery"))).count();
        require(tracked == 2, "Recovered shell did not track both clients")?;
        receipts.push(json!({"held":held,"killedShellPid":old_pid,"restartedShellPid":f.processes[shell].0.id(),"recoverySeconds":elapsed}));
    }
    f.call("grim", &[f.options.output.join("recovered.png").to_str().ok_or("Capture path UTF-8")?])?;
    save(&f.options.output.join("result.json"),&json!({"passed":true,"cycles":receipts,
        "checks":["actual SIGKILL while menu and Dock hold capture","dirty GTK documents survive both crashes",
        "client process and compositor container identities retained","native keyboard works during outage and after recovered capture",
        "menu usable within five seconds after explicit restart"],
        "scope":"Private compositor and explicit supervisor restart; no actual systemd user manager"}))?;
    Ok(())
}

use std::os::unix::process::ExitStatusExt;
