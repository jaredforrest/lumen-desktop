//! Dispatch media keys to one capable, non-relay session player.
use std::{collections::HashMap,env,time::Duration};
use zbus::zvariant::OwnedValue;

const PATH:&str="/org/mpris/MediaPlayer2";
const PLAYER:&str="org.mpris.MediaPlayer2.Player";
type Result<T>=std::result::Result<T,String>;

fn field<T:TryFrom<OwnedValue>>(values:&HashMap<String,OwnedValue>,key:&str)->Option<T>{
    values.get(key)?.try_clone().ok()?.try_into().ok()
}

fn capable(properties:&HashMap<String,OwnedValue>,action:&str)->bool {
    if field::<bool>(properties,"CanControl")!=Some(true){return false;}
    match action {
        "next"=>field::<bool>(properties,"CanGoNext")==Some(true),
        "previous"=>field::<bool>(properties,"CanGoPrevious")==Some(true),
        "play-pause"=>{
            let playing=field::<String>(properties,"PlaybackStatus").as_deref()==Some("Playing");
            field::<bool>(properties,if playing{"CanPause"}else{"CanPlay"})==Some(true)
        }
        _=>false,
    }
}

fn playing(properties:&HashMap<String,OwnedValue>)->bool {
    field::<String>(properties,"PlaybackStatus").as_deref()==Some("Playing")
}
fn eligible_name(name:&str)->bool {
    name.starts_with("org.mpris.MediaPlayer2.") && name!="org.mpris.MediaPlayer2.playerctld"
}

async fn dispatch(action:&str)->Result<bool> {
    let connection=zbus::Connection::session().await.map_err(|e|e.to_string())?;
    let bus=zbus::Proxy::new(&connection,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus")
        .await.map_err(|e|e.to_string())?;
    let mut names:Vec<String>=tokio::time::timeout(Duration::from_secs(2),bus.call("ListNames",&()))
        .await.map_err(|_|"Media session lookup timed out")?.map_err(|e|e.to_string())?;
    names.sort();
    let mut candidates=Vec::new();
    for name in names {
        if !eligible_name(&name) {continue;}
        let owner:String=match tokio::time::timeout(Duration::from_secs(2),bus.call("GetNameOwner",&(&name,))).await {
            Ok(Ok(value))=>value,_=>continue,
        };
        let properties=zbus::Proxy::new(&connection,owner.as_str(),PATH,"org.freedesktop.DBus.Properties")
            .await.map_err(|e|e.to_string())?;
        let values:HashMap<String,OwnedValue>=match tokio::time::timeout(Duration::from_secs(2),properties.call("GetAll",&(PLAYER,))).await {
            Ok(Ok(value))=>value,_=>continue,
        };
        if capable(&values,action) {candidates.push((!playing(&values),name,owner));}
    }
    candidates.sort();
    let method=match action {"next"=>"Next","previous"=>"Previous",_=>"PlayPause"};
    for (_,_,owner) in candidates {
        let player=zbus::Proxy::new(&connection,owner.as_str(),PATH,PLAYER).await.map_err(|e|e.to_string())?;
        match tokio::time::timeout(Duration::from_secs(2),player.call::<_,_,()>(method,&())).await {
            Ok(Ok(()))=>return Ok(true),
            Ok(Err(error)) if missing(&error)=>continue,
            Ok(Err(error))=>return Err(error.to_string()),
            Err(_)=>return Err("Media command timed out".into()),
        }
    }
    Ok(false)
}

fn missing(error:&zbus::Error)->bool {
    match error {
        zbus::Error::MethodError(name,_,_)=>["org.freedesktop.DBus.Error.ServiceUnknown",
            "org.freedesktop.DBus.Error.NameHasNoOwner","org.freedesktop.DBus.Error.UnknownObject"]
            .contains(&name.as_str()),
        _=>false,
    }
}

#[tokio::main]
async fn main() {
    let args:Vec<String>=env::args().skip(1).collect();
    if args.len()!=1 || !["play-pause","next","previous"].contains(&args[0].as_str()) {
        eprintln!("Usage: lumen-media-key play-pause|next|previous");std::process::exit(2);
    }
    match tokio::time::timeout(Duration::from_secs(8),dispatch(&args[0])).await {
        Ok(Ok(_))=>(),
        Ok(Err(error))=>{eprintln!("Media command failed: {error}");std::process::exit(1);},
        Err(_)=>{eprintln!("Media command failed: session timeout");std::process::exit(1);},
    }
}

#[cfg(test)] mod tests {
    use super::*;
    use zbus::zvariant::Value;
    fn properties(status:&str)->HashMap<String,OwnedValue> {
        HashMap::from([
            ("CanControl".into(),OwnedValue::from(true)),
            ("CanPause".into(),OwnedValue::from(true)),
            ("CanPlay".into(),OwnedValue::from(true)),
            ("CanGoNext".into(),OwnedValue::from(true)),
            ("CanGoPrevious".into(),OwnedValue::from(true)),
            ("PlaybackStatus".into(),OwnedValue::try_from(Value::from(status)).unwrap()),
        ])
    }
    #[test] fn capabilities_gate_each_action(){
        let mut player=properties("Playing");
        assert!(capable(&player,"play-pause")&&capable(&player,"next"));
        player.insert("CanPause".into(),OwnedValue::from(false));
        assert!(!capable(&player,"play-pause"));
        player.insert("CanGoPrevious".into(),OwnedValue::from(false));
        assert!(!capable(&player,"previous"));
        player.insert("CanControl".into(),OwnedValue::from(false));
        assert!(!capable(&player,"next"));
        assert!(!capable(&player,"delete"));
    }
    #[test] fn playing_has_priority(){
        assert!(playing(&properties("Playing")));
        assert!(!playing(&properties("Paused")));
    }
    #[test] fn relay_player_is_excluded(){
        assert!(eligible_name("org.mpris.MediaPlayer2.real"));
        assert!(!eligible_name("org.mpris.MediaPlayer2.playerctld"));
        assert!(!eligible_name("unrelated"));
    }
}
