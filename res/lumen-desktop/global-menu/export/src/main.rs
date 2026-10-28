//! DBusMenu exporter for the opt-in Electron adapter.
use futures_util::StreamExt;
use serde_json::{Value,json};
use std::collections::{HashMap,HashSet};
use std::io::Write;
use std::sync::{Arc,Mutex};
use tokio::io::{AsyncBufReadExt,BufReader};
use zbus::{Connection,interface,zvariant::{OwnedObjectPath,OwnedValue,Value as DbusValue}};

const MENU:&str="com.canonical.dbusmenu";
const REG:&str="com.canonical.AppMenu.Registrar";
const REG_PATH:&str="/com/canonical/AppMenu/Registrar";
type Layout=(i32,HashMap<String,OwnedValue>,Vec<OwnedValue>);
#[derive(Clone)] struct Export {xid:u32,state:Arc<Mutex<State>>}
struct State {tree:Value,revision:u32}
fn emit(value:Value) {
    let stdout=std::io::stdout();let mut lock=stdout.lock();
    let _=writeln!(lock,"{value}");let _=lock.flush();
}
fn property(value:&Value)->Option<OwnedValue> {
    match value {
        Value::Bool(value)=>Some(OwnedValue::from(*value)),
        Value::Number(value)=>value.as_i64().and_then(|number|i32::try_from(number).ok()).map(OwnedValue::from),
        Value::String(value)=>OwnedValue::try_from(DbusValue::new(value.clone())).ok(),
        Value::Array(rows)=>{
            let shortcuts=rows.iter().map(|row|row.as_array().map(|row|row.iter().map(|item|item.as_str().map(str::to_owned)).collect::<Option<Vec<_>>>())).collect::<Option<Option<Vec<_>>>>().flatten()?;
            OwnedValue::try_from(DbusValue::new(shortcuts)).ok()
        }
        _=>None,
    }
}
fn layout(node:&Value,depth:usize,budget:&mut usize)->Option<Layout> {
    if depth>64 || *budget==0{return None;}*budget-=1;
    let id=i32::try_from(node["id"].as_i64()?).ok()?;
    let properties=node["props"].as_object().map(|props|props.iter().filter_map(|(key,value)|property(value).map(|value|(key.clone(),value))).collect()).unwrap_or_default();
    let mut children=Vec::new();
    for child in node["children"].as_array().into_iter().flatten() {
        let child=layout(child,depth+1,budget)?;
        children.push(OwnedValue::try_from(DbusValue::new(child)).ok()?);
    }
    Some((id,properties,children))
}
#[interface(name="com.canonical.dbusmenu")]
impl Export {
    fn get_layout(&self,_parent:i32,_depth:i32,_properties:Vec<String>)->zbus::fdo::Result<(u32,Layout)> {
        let state=self.state.lock().map_err(|_|zbus::fdo::Error::Failed("Menu unavailable".into()))?;
        let layout=layout(&state.tree,0,&mut 10000).ok_or_else(||zbus::fdo::Error::Failed("Invalid menu layout".into()))?;
        Ok((state.revision,layout))
    }
    fn about_to_show(&self,_ident:i32)->bool{false}
    fn event(&self,ident:i32,event:String,_data:OwnedValue,_timestamp:u32) {
        if event=="clicked"{emit(json!({"xid":self.xid,"id":ident}));}
    }
}
async fn register(conn:&Connection,xid:u32,path:&str) {
    let Ok(proxy)=zbus::Proxy::new(conn,REG,REG_PATH,REG).await else{return;};
    let Ok(object_path)=OwnedObjectPath::try_from(path) else{return;};
    if proxy.call::<_,_,()>("RegisterWindow",&(xid,object_path)).await.is_ok(){emit(json!({"registered":true,"xid":xid}));}
}
async fn unregister(conn:&Connection,xid:u32) {
    if let Ok(proxy)=zbus::Proxy::new(conn,REG,REG_PATH,REG).await {
        let _=proxy.call::<_,_,()>("UnregisterWindow",&(xid,)).await;
    }
}
async fn apply(conn:&Connection,exports:&mut HashMap<u32,(String,Arc<Mutex<State>>)>,data:&Value)->Result<(),String> {
    let windows=data.as_array().ok_or("Expected a list of windows")?;
    let mut seen=HashSet::new();
    for window in windows {
        let xid=window["xid"].as_u64().and_then(|value|u32::try_from(value).ok()).ok_or("Invalid window ID")?;
        let tree=window.get("menu").ok_or("Missing menu")?.clone();
        if !seen.insert(xid){return Err("Duplicate window ID".into());}
        if !exports.contains_key(&xid) {
            let path=format!("/adapter/window{xid}");
            let state=Arc::new(Mutex::new(State{tree:json!({"id":0}),revision:0}));
            conn.object_server().at(path.as_str(),Export{xid,state:state.clone()}).await.map_err(|e|e.to_string())?;
            exports.insert(xid,(path.clone(),state));
            register(conn,xid,&path).await;
        }
        let (path,state)=exports.get(&xid).ok_or("Missing menu export")?;
        let revision={let mut state=state.lock().map_err(|_|"Menu unavailable")?;
            state.tree=tree;state.revision=state.revision.wrapping_add(1);state.revision};
        conn.emit_signal(None::<&str>,path.as_str(),MENU,"LayoutUpdated",&(revision,0_i32)).await.map_err(|e|e.to_string())?;
    }
    let removed=exports.keys().copied().filter(|xid|!seen.contains(xid)).collect::<Vec<_>>();
    for xid in removed {
        if let Some((path,_))=exports.remove(&xid){
            unregister(conn,xid).await;
            let _=conn.object_server().remove::<Export,_>(path.as_str()).await;
        }
    }
    Ok(())
}
async fn run()->Result<(),String> {
    let conn=Connection::session().await.map_err(|e|e.to_string())?;
    let bus=zbus::Proxy::new(&conn,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus").await.map_err(|e|e.to_string())?;
    let mut owners=bus.receive_signal("NameOwnerChanged").await.map_err(|e|e.to_string())?;
    let mut exports=HashMap::new();
    let mut lines=BufReader::new(tokio::io::stdin()).lines();
    loop {tokio::select! {
        line=lines.next_line()=>match line.map_err(|e|e.to_string())? {
            Some(line)=>match serde_json::from_str::<Value>(&line) {
                Ok(data)=>if let Err(error)=apply(&conn,&mut exports,&data).await{eprintln!("{error}");},
                Err(error)=>eprintln!("{error}"),
            },
            None=>break,
        },
        message=owners.next()=>if let Some(message)=message {
            if let Ok((name,_old,new))=message.body().deserialize::<(String,String,String)>() {
                if name==REG {
                    if new.is_empty(){emit(json!({"registered":false}));}
                    else{for (xid,(path,_)) in &exports{register(&conn,*xid,path).await;}}
                }
            }
        }
    }}
    Ok(())
}
#[tokio::main]
async fn main() {if let Err(error)=run().await{eprintln!("{error}");std::process::exit(1);}}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn tree_has_dbusmenu_layout_signature_and_shortcuts() {
        let node=json!({"id":0,"props":{},"children":[{"id":7,"props":{"label":"Save","enabled":true,"shortcut":[["Control","S"]]},"children":[]}]});
        let root_layout=layout(&node,0,&mut 100).unwrap();
        assert_eq!(root_layout.0,0);assert_eq!(root_layout.2.len(),1);
        assert_eq!(DbusValue::new(root_layout).value_signature().to_string(),"(ia{sv}av)");
        let child=layout(&node["children"][0],0,&mut 100).unwrap();
        assert_eq!(child.1["shortcut"].value_signature().to_string(),"aas");
    }
}
