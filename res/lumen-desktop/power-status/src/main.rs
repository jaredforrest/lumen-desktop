//! A read-only UPower aggregate snapshot. Unknown evidence clears the percentage.
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

#[derive(Default)]
struct Properties {
    present: Option<bool>,
    supply: Option<bool>,
    kind: Option<u32>,
    percentage: Option<f64>,
    state: Option<u32>,
    full: Option<i64>,
    empty: Option<i64>,
}

fn unavailable() -> Value {
    json!({"available":false,"present":false,"percentage":null,"state":"unavailable","low":false,"secondsRemaining":null})
}

fn normalize(properties: &Properties) -> Option<Value> {
    let present = properties.present?;
    if !present { return Some(json!({"available":true,"present":false,"percentage":null,"state":"not-present","low":false,"secondsRemaining":null})); }
    let kind = properties.kind?;
    let supply = properties.supply?;
    if ![2,3].contains(&kind) || !supply { return Some(json!({"available":true,"present":false,"percentage":null,"state":"not-present","low":false,"secondsRemaining":null})); }
    let percentage = properties.percentage?;
    let state = properties.state?;
    if !percentage.is_finite() || !(0.0..=100.0).contains(&percentage) { return None; }
    let state_name = match state {
        0 => "unknown", 1 => "charging", 2 => "discharging", 3 => "empty",
        4 => "charged", 5 => "pending-charge", 6 => "pending-discharge", _ => return None,
    };
    let remaining = match state { 1 => properties.full.unwrap_or(0), 2 => properties.empty.unwrap_or(0), _ => 0 };
    if remaining < 0 { return None; }
    Some(json!({"available":true,"present":true,"percentage":percentage,"state":state_name,
        "low":matches!(state,2|3|6) && percentage<=20.0,
        "secondsRemaining":if remaining>0 {Some(remaining)} else {None}}))
}

fn property<T: TryFrom<OwnedValue>>(items:&HashMap<String,OwnedValue>, key:&str) -> Option<T> {
    items.get(key)?.try_clone().ok()?.try_into().ok()
}

fn decode(items:&HashMap<String,OwnedValue>) -> Properties {
    Properties {
        present:property(items,"IsPresent"), supply:property(items,"PowerSupply"),
        kind:property(items,"Type"), percentage:property(items,"Percentage"),
        state:property(items,"State"), full:property(items,"TimeToFull"), empty:property(items,"TimeToEmpty"),
    }
}

async fn snapshot() -> Result<Value,String> {
    let conn=zbus::Connection::system().await.map_err(|e|e.to_string())?;
    let bus=zbus::Proxy::new(&conn,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus").await.map_err(|e|e.to_string())?;
    let name="org.freedesktop.UPower";
    let owner:String=bus.call("GetNameOwner",&(name,)).await.map_err(|e|e.to_string())?;
    if !owner.starts_with(':') {return Err("No UPower owner".into());}
    let root=zbus::Proxy::new(&conn,owner.as_str(),"/org/freedesktop/UPower","org.freedesktop.UPower").await.map_err(|e|e.to_string())?;
    let path:OwnedObjectPath=root.call("GetDisplayDevice",&()).await.map_err(|e|e.to_string())?;
    let device=zbus::Proxy::new(&conn,owner.as_str(),path.as_str(),"org.freedesktop.DBus.Properties").await.map_err(|e|e.to_string())?;
    let properties:HashMap<String,OwnedValue>=device.call("GetAll",&("org.freedesktop.UPower.Device",)).await.map_err(|e|e.to_string())?;
    let current:String=bus.call("GetNameOwner",&(name,)).await.map_err(|e|e.to_string())?;
    if current!=owner {return Err("UPower owner changed".into());}
    normalize(&decode(&properties)).ok_or("Invalid UPower status".into())
}

#[tokio::main]
async fn main() {
    let result=tokio::time::timeout(Duration::from_secs(3),snapshot()).await.ok()
        .and_then(Result::ok).unwrap_or_else(unavailable);
    println!("{result}");
}

#[cfg(test)] mod tests {
    use super::*;
    fn values() -> Properties {Properties{present:Some(true),supply:Some(true),kind:Some(2),percentage:Some(42.),state:Some(2),empty:Some(5400),full:Some(1200)}}
    #[test] fn charging_discharging_and_low_status() {
        let mut p=values();
        assert_eq!(normalize(&p).unwrap()["secondsRemaining"],5400);
        p.state=Some(1);assert_eq!(normalize(&p).unwrap()["secondsRemaining"],1200);
        p.state=Some(2);p.percentage=Some(12.);
        assert_eq!(normalize(&p).unwrap()["low"],true);
        p.state=Some(3);p.percentage=Some(0.);
        let result=normalize(&p).unwrap();assert_eq!(result["state"],"empty");assert!(result["secondsRemaining"].is_null());
    }
    #[test] fn absent_and_non_supply_hide_percentage() {
        let mut p=values();p.present=Some(false);
        assert_eq!(normalize(&p).unwrap()["percentage"],Value::Null);
        p.present=Some(true);p.supply=Some(false);
        assert_eq!(normalize(&p).unwrap()["present"],false);
        p.supply=Some(true);p.kind=Some(5);
        assert_eq!(normalize(&p).unwrap()["present"],false);
    }
    #[test] fn invalid_values_fail_closed() {
        let mut p=values();p.percentage=Some(f64::NAN);assert!(normalize(&p).is_none());
        p.percentage=Some(101.);assert!(normalize(&p).is_none());
        p.percentage=Some(42.);p.state=Some(7);assert!(normalize(&p).is_none());
        p.state=Some(2);p.empty=Some(-1);assert!(normalize(&p).is_none());
        p.empty=Some(0);assert!(normalize(&p).unwrap()["secondsRemaining"].is_null());
    }
}
