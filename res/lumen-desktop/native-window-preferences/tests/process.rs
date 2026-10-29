//! Persisted native window preferences without changing unrelated application state.
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {root:PathBuf}
impl Fixture {
    fn new()->Self{
        let nonce=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root=std::env::temp_dir().join(format!("lumen-window-preferences-{}-{nonce}",std::process::id()));
        fs::create_dir(&root).unwrap();Self{root}
    }
    fn run(&self,app:&str,args:&[&str]){
        let result=Command::new(env!("CARGO_BIN_EXE_lumen-native-window-preferences"))
            .arg(app).args(args).env("XDG_CONFIG_HOME",&self.root).output().unwrap();
        assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
    }
    fn read(&self,path:&str)->Value{serde_json::from_slice(&fs::read(self.root.join(path)).unwrap()).unwrap()}
}
impl Drop for Fixture{fn drop(&mut self){let _=fs::remove_dir_all(&self.root);}}

#[test]
fn brave_preserves_other_state_across_profiles(){
    let fixture=Fixture::new();
    for profile in ["Default","Profile 1"]{
        let path=fixture.root.join("BraveSoftware/Brave-Browser").join(profile).join("Preferences");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path,json!({"browser":{"custom_chrome_frame":true,"other":7},"session":{"fixture":"retain"}}).to_string()).unwrap();
    }
    fixture.run("brave",&[]);
    for profile in ["Default","Profile 1"]{
        let value=fixture.read(&format!("BraveSoftware/Brave-Browser/{profile}/Preferences"));
        assert_eq!(value["browser"],json!({"custom_chrome_frame":false,"other":7}));
        assert_eq!(value["session"],json!({"fixture":"retain"}));
        assert_eq!(value["extensions"]["theme"]["system_theme"],1);
        assert_eq!(value["brave"]["wallet"]["show_wallet_icon_on_toolbar"],false);
    }
    assert_eq!(fixture.read("BraveSoftware/Brave-Browser/Local State")["brave"]["tabs"]["compact_horizontal_tabs"],true);
}

#[test]
fn gimp_preserves_editing_settings_and_repeat_is_idempotent(){
    let fixture=Fixture::new();
    let path=fixture.root.join("GIMP/3.2/gimprc");fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path,"# retained comment\n(theme-color-scheme dark)\n(undo-levels 24)\n").unwrap();
    fixture.run("gimp",&[]);
    assert_eq!(fs::read_to_string(&path).unwrap(),"# retained comment\n(theme-color-scheme system)\n(undo-levels 24)\n");
    let before=fs::read(&path).unwrap();fixture.run("gimp",&[]);assert_eq!(fs::read(&path).unwrap(),before);
}

#[test]
fn active_obsidian_is_untouched_then_receives_native_frame(){
    let fixture=Fixture::new();
    let root=fixture.root.join("obsidian");fs::create_dir(&root).unwrap();
    let path=root.join("obsidian.json");fs::write(&path,"{\"frame\":\"hidden\",\"vaults\":{\"fixture\":{}}}").unwrap();
    symlink("fixture-12345",root.join("SingletonLock")).unwrap();
    let before=fs::read(&path).unwrap();fixture.run("obsidian",&[]);assert_eq!(fs::read(&path).unwrap(),before);
    fs::remove_file(root.join("SingletonLock")).unwrap();fixture.run("obsidian",&[]);
    assert_eq!(fixture.read("obsidian/obsidian.json"),json!({"frame":"native","vaults":{"fixture":{}}}));
}

#[test]
fn explicit_browser_profile_stays_outside_default_tree(){
    let fixture=Fixture::new();let custom=fixture.root.join("custom-profile");
    fixture.run("brave",&["--user-data-dir",custom.to_str().unwrap()]);
    assert!(!fixture.root.join("BraveSoftware").exists());
    assert_eq!(fixture.read("custom-profile/Default/Preferences")["browser"]["custom_chrome_frame"],false);
}

#[test]
fn stale_lock_after_reboot_allows_frame_setting(){
    let fixture=Fixture::new();let root=fixture.root.join("obsidian");fs::create_dir(&root).unwrap();
    let hostname=fs::read_to_string("/proc/sys/kernel/hostname").unwrap();
    symlink(format!("{}-2147483646",hostname.trim()),root.join("SingletonLock")).unwrap();
    fixture.run("obsidian",&[]);
    assert_eq!(fixture.read("obsidian/obsidian.json")["frame"],"native");
}
