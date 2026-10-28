use lumen_application_lifecycle::{Lifecycle,ObservationStore,process_identity};
use lumen_application_lifecycle::session::{Prepared,QuitProtocol,SessionEnd};
use serde_json::{Value,json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::Mutex;

struct FakeQuit {calls:Mutex<Vec<String>>}
impl QuitProtocol for FakeQuit {
    fn prepare(&self,lifecycle:&mut Lifecycle,app:&str)->Result<Prepared,String>{
        let identity=lifecycle.exact_process(app)?.identity.clone();
        self.calls.lock().unwrap().push(format!("prepare:{app}"));
        Ok(Prepared{app:app.into(),identity,owner:format!(":fixture.{app}")})
    }
    fn dispatch(&self,lifecycle:&mut Lifecycle,prepared:&Prepared)->Result<(),String>{
        if lifecycle.exact_process(&prepared.app)?.identity!=prepared.identity{return Err("changed".into());}
        self.calls.lock().unwrap().push(format!("dispatch:{}",prepared.app));Ok(())
    }
}
#[test]
fn live_process_survives_window_close_and_session_requires_exit(){
    let mut child=Command::new("sleep").arg("30").spawn().unwrap();
    let app="org.example.Lifecycle";
    let node=json!({"id":3,"shell":"xdg_shell","window":null,"pid":child.id(),"app_id":app});
    let mut lifecycle=Lifecycle::default();
    lifecycle.refresh("epoch",std::slice::from_ref(&node));
    assert_eq!(lifecycle.rows()[0]["windowCount"],1);
    lifecycle.refresh("epoch",&[]);
    assert_eq!(lifecycle.rows()[0]["zeroWindows"],true);
    assert_eq!(lifecycle.exact_process(app).unwrap().identity,process_identity(child.id()).unwrap());
    lifecycle.refresh("epoch",std::slice::from_ref(&node));
    let protocol=FakeQuit{calls:Mutex::new(Vec::new())};
    let mut session=SessionEnd::new(0);
    assert_eq!(session.begin("logout",std::slice::from_ref(&node),&mut lifecycle,&protocol).unwrap()["phase"],"waiting");
    assert_eq!(*protocol.calls.lock().unwrap(),vec![format!("prepare:{app}"),format!("dispatch:{app}")]);
    let token=session.token.clone();
    assert!(session.authorize(&token,std::slice::from_ref(&node),&mut lifecycle,&protocol).is_err());
    child.kill().unwrap();let _=child.wait().unwrap();
    lifecycle.prune();session.tick(&[],&mut lifecycle,&protocol);
    assert_eq!(session.state()["phase"],"ready");
    assert_eq!(session.authorize(&token,&[],&mut lifecycle,&protocol).unwrap()["action"],"logout");
    assert!(session.authorize(&token,&[],&mut lifecycle,&protocol).is_err());
}

#[test]
fn recovery_is_private_scoped_and_single_lease(){
    let root=std::env::temp_dir().join(format!("lumen-lifecycle-recovery-{}",std::process::id()));
    fs::create_dir(&root).unwrap();fs::set_permissions(&root,fs::Permissions::from_mode(0o700)).unwrap();
    let epoch="0123456789abcdef01234567";let boot="01234567-89ab-cdef-0123-456789abcdef";
    let mut store=ObservationStore::new(&root,epoch,boot).unwrap();
    assert!(ObservationStore::new(&root,epoch,boot).is_err());
    store.save(&[]).unwrap();
    assert_eq!(store.load(),Vec::<Value>::new());
    let record=store.directory.join(format!("{epoch}.json"));
    assert_eq!(fs::metadata(&record).unwrap().permissions().mode()&0o777,0o600);
    fs::write(&record,b"malformed").unwrap();
    assert!(store.load().is_empty());
    assert!(!store.warning.is_empty());
    let outside=root.join("unrelated");fs::write(&outside,b"private").unwrap();fs::remove_file(&record).unwrap();
    std::os::unix::fs::symlink(&outside,&record).unwrap();
    assert!(store.load().is_empty());
    assert_eq!(fs::read(&outside).unwrap(),b"private");
    drop(store);fs::remove_dir_all(root).unwrap();
}
