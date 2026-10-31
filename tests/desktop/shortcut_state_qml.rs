//! Real QML reloads effective shortcuts and fails closed after file loss.
mod private_qml_root;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

unsafe extern "C" { fn kill(pid: i32, signal: i32) -> i32; }

fn publish(source:&std::path::Path,chord:&str) {
    let declaration=source.with_extension("declaration");
    fs::write(&declaration,format!("{{\"version\":1,\"bindings\":[{{\"id\":\"test\",\"action\":\"window.fullscreen.toggle\",\"chord\":\"{chord}\"}}]}}")).unwrap();
    let value=Command::new("lumen-shortcuts").arg(&declaration).output().unwrap();
    assert!(value.status.success(),"{}",String::from_utf8_lossy(&value.stderr));
    let temporary=source.with_extension("tmp");
    fs::write(&temporary,value.stdout).unwrap();
    fs::rename(temporary,source).unwrap();
}
fn wait(log:&std::path::Path,child:&mut Child,expected:&str) {
    let until=Instant::now()+Duration::from_secs(8);
    loop {
        let output=fs::read_to_string(log).unwrap_or_default();
        if output.lines().filter_map(|line|line.split_once("SHORTCUT_STATE ").map(|(_,state)|state))
            .last().is_some_and(|state|state.contains(expected)) {return;}
        if child.try_wait().unwrap().is_some() || Instant::now()>=until {panic!("{output}");}
        thread::sleep(Duration::from_millis(30));
    }
}

#[test]
fn shortcut_state_reloads_and_fails_closed() {
    let root = private_qml_root::create().unwrap();
    let result=(||{
        let source_path=std::path::Path::new(file!()).canonicalize()?;
        let shell=source_path.parent().unwrap().parent().unwrap().parent().unwrap().join("res/lumen-desktop/shell");
        fs::write(root.join("shell.qml"),include_str!("shortcut_state_fixture.qml").replace("LUMEN_ASSETS",&format!("file:{}",shell.display())))?;
        let config=root.join("config");fs::create_dir_all(config.join("lumen-desktop"))?;
        let source=config.join("lumen-desktop/shortcuts.json");
        publish(&source,"Mod4+Control+f");
        let runtime=root.join("runtime");fs::create_dir(&runtime)?;
        fs::set_permissions(&runtime,fs::Permissions::from_mode(0o700))?;
        let home=root.join("home");fs::create_dir(&home)?;
        let log=root.join("qt.log");
        let output=fs::File::create(&log)?;
        let mut child=Command::new("quickshell").args(["-p",root.to_str().unwrap()])
            .process_group(0).env("HOME",home).env("XDG_RUNTIME_DIR",runtime)
            .env("XDG_CONFIG_HOME",config).env("XDG_DATA_HOME",root.join("data"))
            .env("XDG_CACHE_HOME",root.join("cache"))
            .env("QT_QPA_PLATFORM","offscreen").env("QT_QPA_PLATFORMTHEME","")
            .env("QT_QUICK_BACKEND","software").env("QT_ACCESSIBILITY","0")
            .env("NO_AT_BRIDGE","1").env("GTK_A11Y","none")
            .env("DBUS_SYSTEM_BUS_ADDRESS",format!("unix:path={}/no-system",root.display()))
            .env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY").env_remove("SWAYSOCK")
            .env_remove("DBUS_SESSION_BUS_ADDRESS").env_remove("AT_SPI_BUS_ADDRESS")
            .stdout(Stdio::from(output.try_clone()?)).stderr(Stdio::from(output)).spawn()?;
        let checked=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            wait(&log,&mut child,"\"available\":true,\"hint\":\"Ctrl+Super+F\",\"bindings\":1");
            publish(&source,"Mod4+Control+x");
            wait(&log,&mut child,"\"available\":true,\"hint\":\"Ctrl+Super+X\",\"bindings\":1");
            fs::write(&source,"{bad").unwrap();
            wait(&log,&mut child,"\"available\":false,\"hint\":\"\",\"bindings\":0");
            publish(&source,"Control+z");
            wait(&log,&mut child,"\"available\":true,\"hint\":\"Ctrl+Z\",\"bindings\":1");
            fs::remove_file(&source).unwrap();
            wait(&log,&mut child,"\"available\":false,\"hint\":\"\",\"bindings\":0");
        }));
        unsafe {kill(-(child.id() as i32),9);}
        let _=child.wait();
        if let Err(failure)=checked {std::panic::resume_unwind(failure);}
        let output=fs::read_to_string(&log)?;
        assert!(output.contains("SHORTCUT_LAYOUT_MODEL passed"),"{output}");
        for marker in ["ReferenceError:","TypeError:","Binding loop","Cannot assign","Failed to load configuration"] {
            assert!(!output.contains(marker),"{output}");
        }
        Ok::<(),std::io::Error>(())
    })();
    fs::remove_dir_all(root).unwrap();
    result.unwrap();
}
