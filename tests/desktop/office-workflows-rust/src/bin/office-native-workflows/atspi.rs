//! Captured private AT-SPI controls for physical office export decisions.
use lumen_office_workflows::{Result, ensure};
use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

pub struct Atspi { bus: Connection }
type Reference = (String,String);
impl Atspi {
    pub fn connect(address: &str) -> Result<Self> {
        ensure(address.starts_with("unix:path=/run/fixture/"),"Private AT-SPI bus required")?;
        Ok(Self { bus:zbus::blocking::connection::Builder::address(address)?.build()? })
    }
    fn proxy<'a>(&'a self, reference: &'a Reference, interface: &'a str) -> Result<Proxy<'a>> {
        Ok(Proxy::new(&self.bus, reference.0.as_str(), reference.1.as_str(), interface)?)
    }
    fn name(&self, reference: &Reference) -> Result<String> {
        let value:OwnedValue=self.proxy(reference,"org.freedesktop.DBus.Properties")?.call("Get", &("org.a11y.atspi.Accessible","Name"))?;
        Ok(String::try_from(value)?)
    }
    fn role(&self, reference: &Reference) -> Result<String> {
        let proxy=self.proxy(reference,"org.a11y.atspi.Accessible")?;
        if let Ok(role)=proxy.call::<_,_,String>("GetRoleName", &()) {return Ok(role);}
        if let Ok(role)=proxy.call::<_,_,u32>("GetRole", &()) {return Ok(format!("role:{role}"));}
        Ok(String::new())
    }
    fn state(&self, reference: &Reference, index: usize) -> Result<bool> {
        let words:Vec<u32>=self.proxy(reference,"org.a11y.atspi.Accessible")?.call("GetState", &())?;
        Ok(words.get(index/32).is_some_and(|word|word&(1<<(index%32))!=0))
    }
    fn pid(&self, reference: &Reference) -> Result<u32> {
        Ok(Proxy::new(&self.bus,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus")?.call("GetConnectionUnixProcessID", &(reference.0.as_str(),))?)
    }
    fn children(&self, reference: &Reference) -> Result<Vec<Reference>> {
        let rows:Vec<(String,OwnedObjectPath)>=self.proxy(reference,"org.a11y.atspi.Accessible")?.call("GetChildren", &())?;
        Ok(rows.into_iter().map(|(owner,path)|(owner,path.to_string())).collect())
    }
    fn reference(row:&Value) -> Result<Reference> {
        let values=row["reference"].as_array().ok_or("Accessible reference")?;
        Ok((values.first().and_then(Value::as_str).ok_or("Accessible owner")?.into(),values.get(1).and_then(Value::as_str).ok_or("Accessible object")?.into()))
    }
    pub fn find(&self, owner_pid:u32, wanted:&str, exact:bool) -> Result<Option<Value>> {
        self.find_roles(owner_pid,wanted,exact,&[])
    }
    pub fn find_roles(&self, owner_pid:u32, wanted:&str, exact:bool, roles:&[&str]) -> Result<Option<Value>> {
        let mut pending=VecDeque::from([("org.a11y.atspi.Registry".to_owned(),"/org/a11y/atspi/accessible/root".to_owned())]);
        let mut seen=HashSet::new();let end=Instant::now()+Duration::from_secs(5);
        while let Some(reference)=pending.pop_front() {
            ensure(seen.len()<2048 && Instant::now()<end,"AT-SPI search exceeded private observation budget")?;
            if reference.1=="/org/a11y/atspi/null" || !seen.insert(reference.clone()) {continue;}
            if let Ok(name)=self.name(&reference) {
                if (if exact {name==wanted} else {name.contains(wanted)}) && self.pid(&reference).ok()==Some(owner_pid) {
                    let role=self.role(&reference)?;
                    let showing=self.state(&reference,25)?;
                    let sensitive=self.state(&reference,24)?;
                    if showing && (roles.is_empty() || roles.contains(&role.as_str())) {return Ok(Some(json!({"reference":[reference.0,reference.1],"pid":owner_pid,"name":name,"role":role,"showing":showing,"sensitive":sensitive})));}
                }
            }
            if let Ok(children)=self.children(&reference) {pending.extend(children);}
        }
        Ok(None)
    }
    pub fn bounds_captured(&self,row:&Value) -> Result<(i32,i32,i32,i32)> {
        let reference=Self::reference(row)?;
        ensure(self.pid(&reference)? as i64==row["pid"].as_i64().ok_or("Captured PID")? && self.name(&reference)?==row["name"] && self.role(&reference)?==row["role"] && self.state(&reference,25)? && self.state(&reference,24)?,"Captured accessible control changed")?;
        let bounds:(i32,i32,i32,i32)=self.proxy(&reference,"org.a11y.atspi.Component")?.call("GetExtents", &(0_u32,))?;
        ensure(bounds.2>0 && bounds.3>0,"Accessible control has no painted extent")?;
        Ok(bounds)
    }
}
