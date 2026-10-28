use serde_json::{Value,json};
use std::collections::HashMap;
use std::env;
use std::io::{BufRead,BufReader,Write};
use std::process::{Command,Stdio};
use std::sync::{Arc,Mutex,mpsc};
use std::time::Duration;
use zbus::{interface,message::Header,zvariant::{OwnedObjectPath,OwnedValue}};

#[derive(Clone,Default)] struct Registry(Arc<Mutex<Option<(String,String,u32)>>>);
#[derive(Clone)] struct Registrar(Registry);
#[interface(name="com.canonical.AppMenu.Registrar")]
impl Registrar {
    fn register_window(&self,xid:u32,path:OwnedObjectPath,#[zbus(header)] header:Header<'_>) {
        *self.0.0.lock().unwrap()=Some((header.sender().unwrap().to_string(),path.to_string(),xid));
    }
    fn unregister_window(&self,_xid:u32){}
}
#[test] fn private_bus() {
    if env::var_os("MENU_EXPORT_CHILD").is_some(){child();return;}
    let mut runner=Command::new("timeout");runner.args(["15","dbus-run-session"]);
    if let Ok(config)=env::var("LUMEN_TEST_DBUS_CONFIG"){runner.arg(format!("--config-file={config}"));}
    let output=runner.arg("--").arg(env::current_exe().unwrap()).args(["--exact","private_bus","--nocapture"])
        .env("MENU_EXPORT_CHILD","1").output().unwrap();
    assert!(output.status.success(),"{}{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
}
fn child() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let registry=Registry::default();
        let conn=zbus::connection::Builder::session().unwrap().name("com.canonical.AppMenu.Registrar").unwrap()
            .serve_at("/com/canonical/AppMenu/Registrar",Registrar(registry.clone())).unwrap().build().await.unwrap();
        let mut exporter=Command::new(env!("CARGO_BIN_EXE_lumen-menu-export"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let stdout=exporter.stdout.take().unwrap();let (tx,rx)=mpsc::channel::<Value>();
        std::thread::spawn(move ||for line in BufReader::new(stdout).lines().flatten(){
            if let Ok(value)=serde_json::from_str(&line){if tx.send(value).is_err(){break;}}
        });
        writeln!(exporter.stdin.as_mut().unwrap(),"{}",json!([{"xid":23,"menu":{"id":0,"props":{},"children":[{"id":7,"props":{"label":"Save","enabled":true,"shortcut":[["Control","S"]]},"children":[]}]}}])).unwrap();
        let registered=rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(registered["registered"],true);
        let (owner,path,xid)=registry.0.lock().unwrap().clone().unwrap();assert_eq!(xid,23);
        let menu=zbus::Proxy::new(&conn,owner.as_str(),path.as_str(),"com.canonical.dbusmenu").await.unwrap();
        let (revision,layout):(u32,(i32,HashMap<String,OwnedValue>,Vec<OwnedValue>))=menu.call("GetLayout",&(0_i32,-1_i32,Vec::<String>::new())).await.unwrap();
        assert_eq!(revision,1);assert_eq!(layout.0,0);assert_eq!(layout.2.len(),1);
        menu.call::<_,_,()>("Event",&(7_i32,"clicked",OwnedValue::from(0_i32),0_u32)).await.unwrap();
        let clicked=rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(clicked["xid"],23);assert_eq!(clicked["id"],7);
        drop(exporter.stdin.take());
        assert!(exporter.wait().unwrap().success());
    });
}
