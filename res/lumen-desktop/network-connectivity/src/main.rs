//! Existing NetworkManager facts; this probe never starts or configures a network.
use serde_json::{Value,json};
use std::{collections::HashMap,env,io::{self,Read},time::Duration};
use zbus::zvariant::{OwnedObjectPath,OwnedValue};

fn field<T:TryFrom<OwnedValue>>(items:&HashMap<String,OwnedValue>,key:&str)->Option<T> {
    items.get(key)?.try_clone().ok()?.try_into().ok()
}

fn normalize(manager:&HashMap<String,OwnedValue>,device:&HashMap<String,OwnedValue>)->Option<Value> {
    if field::<bool>(device,"Managed")? == false {return None;}
    if !field::<bool>(manager,"WirelessEnabled")? || !field::<bool>(manager,"WirelessHardwareEnabled")? {
        return Some(json!({"phase":"disabled","connectivity":"unknown"}));
    }
    let phase=match field::<u32>(device,"State")? {
        40|50|60=>"associating",70|80|90=>"acquiring",100=>"connected",
        110=>"disconnecting",30|120=>"disconnected",20=>"unavailable",0|10=>return None,_=>return None,
    };
    let mut connectivity="unknown";
    if phase=="connected" && field::<bool>(manager,"ConnectivityCheckAvailable").unwrap_or(false)
        && field::<bool>(manager,"ConnectivityCheckEnabled").unwrap_or(false) {
        let v4=field::<u32>(device,"Ip4Connectivity").unwrap_or(0);
        let v6=field::<u32>(device,"Ip6Connectivity").unwrap_or(0);
        if v4>4 || v6>4 {return None;}
        connectivity=if v4==4 || v6==4 {"full"} else if v4==2 || v6==2 {"portal"}
            else if v4==3 || v6==3 {"limited"} else if v4==1 && v6==1 {"none"} else {"unknown"};
    }
    Some(json!({"phase":phase,"connectivity":connectivity}))
}

async fn get_all(conn:&zbus::Connection,owner:&str,path:&str,interface:&str)->Result<HashMap<String,OwnedValue>,String> {
    let proxy=zbus::Proxy::new(conn,owner,path,"org.freedesktop.DBus.Properties").await.map_err(|e|e.to_string())?;
    proxy.call("GetAll",&(interface,)).await.map_err(|e|e.to_string())
}

async fn report(interface:Option<&str>)->Result<Option<Value>,String> {
    let conn=zbus::Connection::system().await.map_err(|e|e.to_string())?;
    let bus=zbus::Proxy::new(&conn,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus").await.map_err(|e|e.to_string())?;
    let service="org.freedesktop.NetworkManager";
    let owner:String=bus.call("GetNameOwner",&(service,)).await.map_err(|e|e.to_string())?;
    if !owner.starts_with(':'){return Ok(None);}
    let manager=get_all(&conn,&owner,"/org/freedesktop/NetworkManager",service).await?;
    let proxy=zbus::Proxy::new(&conn,owner.as_str(),"/org/freedesktop/NetworkManager",service).await.map_err(|e|e.to_string())?;
    let paths:Vec<OwnedObjectPath>=proxy.call("GetDevices",&()).await.map_err(|e|e.to_string())?;
    if paths.len()>128{return Ok(None);}
    let mut candidates=Vec::new();
    for path in paths {
        let device=get_all(&conn,&owner,path.as_str(),"org.freedesktop.NetworkManager.Device").await?;
        if field::<u32>(&device,"DeviceType")!=Some(2){continue;}
        let Some(name)=field::<String>(&device,"Interface") else {continue;};
        if interface.is_some_and(|expected|expected!=name){continue;}
        candidates.push((name,device));
    }
    if candidates.len()!=1 {return Ok(None);}
    let (name,device)=candidates.pop().unwrap();
    let Some(mut normalized)=normalize(&manager,&device) else{return Ok(None);};
    let current:String=bus.call("GetNameOwner",&(service,)).await.map_err(|e|e.to_string())?;
    if current!=owner{return Ok(None);}
    normalized["interface"]=json!(name);
    normalized["source"]=json!("network-manager");
    Ok(Some(normalized))
}

fn merge(mut wireless:Value,report:Option<Value>)->Value {
    if !wireless.is_object(){wireless=json!({});}
    let available=wireless["available"].as_bool().unwrap_or(false);
    let state=wireless["state"].as_str().unwrap_or("UNKNOWN");
    let phase=match state {
        "COMPLETED"=>"connected","DISCONNECTED"|"INACTIVE"=>"disconnected","INTERFACE_DISABLED"=>"disabled",
        "SCANNING"|"ASSOCIATING"|"ASSOCIATED"|"AUTHENTICATING"|"4WAY_HANDSHAKE"|"GROUP_HANDSHAKE"=>"associating",
        _=>"unknown",
    };
    wireless["controlAvailable"]=json!(available);
    wireless["phase"]=json!(if available {phase}else{"service-unavailable"});
    wireless["connectivity"]=json!("unknown");
    wireless["statusSource"]=json!(if available {"supplicant"}else{"unavailable"});
    if let Some(report)=report {
        for (target,source) in [("phase","phase"),("connectivity","connectivity"),("statusSource","source"),("interface","interface")] {
            wireless[target]=report[source].clone();
        }
    }
    wireless
}

#[tokio::main]
async fn main(){
    let mut input=String::new();
    let _=io::stdin().take(65537).read_to_string(&mut input);
    let wireless=serde_json::from_str(&input).unwrap_or_else(|_|json!({"available":false,"networks":[]}));
    let interface=env::args().nth(1);
    let report=tokio::time::timeout(Duration::from_secs(3),report(interface.as_deref())).await.ok().and_then(Result::ok).flatten();
    println!("{}",merge(wireless,report));
}

#[cfg(test)] mod tests {
    use super::*;
    fn props(items:&[(&str,OwnedValue)])->HashMap<String,OwnedValue>{items.iter().map(|(key,value)|((*key).to_owned(),value.try_clone().unwrap())).collect()}
    #[test] fn managed_wifi_phase_and_connectivity() {
        let manager=props(&[("WirelessEnabled",OwnedValue::from(true)),("WirelessHardwareEnabled",OwnedValue::from(true)),("ConnectivityCheckAvailable",OwnedValue::from(true)),("ConnectivityCheckEnabled",OwnedValue::from(true))]);
        let device=props(&[("Managed",OwnedValue::from(true)),("State",OwnedValue::from(100_u32)),("Ip4Connectivity",OwnedValue::from(4_u32)),("Ip6Connectivity",OwnedValue::from(2_u32))]);
        assert_eq!(normalize(&manager,&device).unwrap()["connectivity"],"full");
        let mut disabled=props(&[("WirelessEnabled",OwnedValue::from(false)),("WirelessHardwareEnabled",OwnedValue::from(true))]);
        assert_eq!(normalize(&disabled,&device).unwrap()["phase"],"disabled");
        disabled.insert("WirelessEnabled".into(),OwnedValue::from(true));
        let unmanaged=props(&[("Managed",OwnedValue::from(false))]);
        assert!(normalize(&disabled,&unmanaged).is_none());
    }
    #[test] fn supplicant_fallback_and_report_merge() {
        let fallback=merge(json!({"available":true,"state":"SCANNING","name":"Fixture"}),None);
        assert_eq!(fallback["phase"],"associating");assert_eq!(fallback["statusSource"],"supplicant");
        let result=merge(fallback,Some(json!({"phase":"connected","connectivity":"portal","source":"network-manager","interface":"wlan0"})));
        assert_eq!(result["connectivity"],"portal");assert_eq!(result["statusSource"],"network-manager");
    }
}
