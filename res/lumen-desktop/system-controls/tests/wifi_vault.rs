use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command, time::{SystemTime,UNIX_EPOCH}};
fn fixture(vault:bool,locked:bool,local:bool)->(PathBuf, std::process::Output){
    let root=std::env::temp_dir().join(format!("wifi-vault-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir(&root).unwrap();fs::create_dir(root.join("bin")).unwrap();
    let nu=std::env::var("PATH").unwrap().split(':').map(|p|PathBuf::from(p).join("nu")).find(|p|p.is_file()).expect("Nushell fixture runtime");
    for (name,source) in [("rbw",include_str!("rbw_fixture.nu")),("nmcli",include_str!("nmcli_fixture.nu"))]{
        let path=root.join("bin").join(name);fs::write(&path,format!("#!{} --stdin\n{source}",nu.display())).unwrap();fs::set_permissions(path,fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(root.join("cached.txt"),"local password 123").unwrap();
    if locked {fs::write(root.join("locked"),"").unwrap();}
    if !local {fs::write(root.join("no-local"),"").unwrap();}
    if vault {
        let item=serde_json::json!({"id":"11111111-1111-1111-1111-111111111111","folder":"Wi-Fi","name":"Wi-Fi — Fixture Wi-Fi [wpa-psk]","data":{"password":"authoritative password"},"notes":"lumen.wifi.v1:{\"ssid\":\"Fixture Wi-Fi\",\"security\":\"wpa-psk\"}"});
        fs::write(root.join("item.json"),item.to_string()).unwrap();
    }
    let output=Command::new(env!("CARGO_BIN_EXE_lumen-system-controls")).arg("wifi-vault-sync")
        .env("PATH",format!("{}:{}",root.join("bin").display(),std::env::var("PATH").unwrap()))
        .env("XDG_STATE_HOME",root.join("state")).env("FIXTURE_ROOT",&root).output().unwrap();
    (root,output)
}
#[test]fn vault_wins_without_secret_output(){
    let (root,out)=fixture(true,false,true);
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stdout));
    assert_eq!(fs::read_to_string(root.join("cached.txt")).unwrap(),"authoritative password");
    assert!(!root.join("uploaded.json").exists());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("authoritative password"));
    fs::remove_dir_all(root).unwrap();
}
#[test]fn missing_network_is_enrolled_without_replacing_cache(){
    let (root,out)=fixture(false,false,true);
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stdout));
    let uploaded:serde_json::Value=serde_json::from_slice(&fs::read(root.join("uploaded.json")).unwrap()).unwrap();
    assert_eq!(uploaded["password"],"local password 123");assert!(uploaded["notes"].as_str().unwrap().starts_with("lumen.wifi.v1:"));
    assert_eq!(fs::read_to_string(root.join("cached.txt")).unwrap(),"local password 123");fs::remove_dir_all(root).unwrap();
}
#[test]fn locked_vault_keeps_offline_cache(){
    let (root,out)=fixture(true,true,true);assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("locked"));
    assert_eq!(fs::read_to_string(root.join("cached.txt")).unwrap(),"local password 123");fs::remove_dir_all(root).unwrap();
}
#[test]fn remote_network_is_cached_without_activation(){
    let (root,out)=fixture(true,false,false);
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stdout));
    assert!(root.join("created-profile").exists());assert!(root.join("autoconnect").exists());
    assert_eq!(fs::read_to_string(root.join("cached.txt")).unwrap(),"authoritative password");
    fs::remove_dir_all(root).unwrap();
}
