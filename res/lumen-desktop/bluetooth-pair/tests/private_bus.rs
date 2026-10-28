use std::env;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command,Stdio};
use std::sync::{Arc,Mutex};
use std::time::Duration;
use zbus::{Connection,interface,message::Header,zvariant::OwnedObjectPath};

const DEVICE:&str="/org/bluez/hci0/dev_00_11_22_33_44_55";
#[derive(Clone,Default)] struct Registry(Arc<Mutex<Option<(String,String)>>>);
#[derive(Clone)] struct Manager(Registry);
#[interface(name="org.bluez.AgentManager1")]
impl Manager {
    async fn register_agent(&self,path:OwnedObjectPath,_capability:String,#[zbus(header)] header:Header<'_>) {
        let sender=header.sender().unwrap().to_string();
        *self.0.0.lock().unwrap()=Some((sender,path.to_string()));
    }
    fn unregister_agent(&self,_path:OwnedObjectPath){*self.0.0.lock().unwrap()=None;}
}
#[derive(Clone)] struct Device(Registry);
#[interface(name="org.bluez.Device1")]
impl Device {
    async fn pair(&self,#[zbus(connection)] connection:&Connection)->zbus::fdo::Result<()> {
        let (owner,path)=self.0.0.lock().unwrap().clone().ok_or_else(||zbus::fdo::Error::Failed("Agent was not registered".into()))?;
        let proxy=zbus::Proxy::new(connection,owner.as_str(),path.as_str(),"org.bluez.Agent1")
            .await.map_err(|e|zbus::fdo::Error::Failed(e.to_string()))?;
        let device=OwnedObjectPath::try_from(DEVICE).unwrap();
        proxy.call::<_,_,()>("RequestConfirmation",&(device,123456_u32)).await
            .map_err(|e|zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }
    fn cancel_pairing(&self) {}
}
#[test] fn private_bus() {
    if env::var_os("PAIR_PRIVATE_CHILD").is_some(){child();return;}
    let mut runner=Command::new("timeout");runner.args(["15","dbus-run-session"]);
    if let Ok(config)=env::var("LUMEN_TEST_DBUS_CONFIG") {runner.arg(format!("--config-file={config}"));}
    let output=runner.arg("--").arg(env::current_exe().unwrap())
        .args(["--exact","private_bus","--nocapture"]).env("PAIR_PRIVATE_CHILD","1")
        .output().expect("private D-Bus session");
    assert!(output.status.success(),"{}{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
}
fn child() {
    let runtime=tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let registry=Registry::default();
        let _conn=zbus::connection::Builder::session().unwrap().name("org.bluez").unwrap()
            .serve_at("/org/bluez",Manager(registry.clone())).unwrap()
            .serve_at(DEVICE,Device(registry.clone())).unwrap()
            .build().await.unwrap();
        let mut process=Command::new(env!("CARGO_BIN_EXE_lumen-bluetooth-pair"))
            .arg(DEVICE).env("DBUS_SYSTEM_BUS_ADDRESS",env::var("DBUS_SESSION_BUS_ADDRESS").unwrap())
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().unwrap();
        let stdout=process.stdout.take().unwrap();
        let result=tokio::task::spawn_blocking(move || {
            let mut reader=BufReader::new(stdout);
            let mut line=String::new();reader.read_line(&mut line).unwrap();
            assert!(line.contains("\"kind\":\"confirm\""),"{line}");
            assert!(line.contains("123456"),"{line}");
            line.clear();reader.read_line(&mut line).unwrap();
            assert!(line.contains("\"event\":\"paired\""),"{line}");
        });
        for _ in 0..100 {
            if registry.0.lock().unwrap().is_some(){break;}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(registry.0.lock().unwrap().is_some());
        process.stdin.as_mut().unwrap().write_all(b"{\"accept\":true}\n").unwrap();
        tokio::time::timeout(Duration::from_secs(5),result).await.unwrap().unwrap();
        assert!(process.wait().unwrap().success());
    });
}
