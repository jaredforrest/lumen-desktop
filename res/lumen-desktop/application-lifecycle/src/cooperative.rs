use crate::{Lifecycle,Result};
use crate::session::{Prepared,QuitProtocol};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::collections::HashMap;
use std::time::Duration;
use zbus::zvariant::{OwnedValue,Signature};

pub struct Cooperative;
fn protocol_id(app:&str)->bool{
    let parts=app.split('.').collect::<Vec<_>>();
    parts.len()>=2&&parts.iter().all(|part|{
        let mut chars=part.chars();
        chars.next().is_some_and(|ch|ch.is_ascii_alphabetic()||ch=='_')&&chars.all(|ch|ch.is_ascii_alphanumeric()||ch=='_'||ch=='-')
    })
}
fn runtime<T>(future:impl Future<Output=Result<T>>)->Result<T>{
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|error|error.to_string())?;
    runtime.block_on(async{tokio::time::timeout(Duration::from_secs(8),future).await.map_err(|_|"Application protocol timed out".to_owned())?})
}
async fn endpoint(app:&str,identity:&Value)->Result<(zbus::Connection,String,String)>{
    let connection=zbus::Connection::session().await.map_err(|_|"Application protocol unavailable")?;
    let bus=zbus::Proxy::new(&connection,"org.freedesktop.DBus","/org/freedesktop/DBus","org.freedesktop.DBus").await.map_err(|_|"Application protocol unavailable")?;
    let owner:String=tokio::time::timeout(Duration::from_secs(2),bus.call("GetNameOwner",&(app,))).await.map_err(|_|"Application protocol timed out")?.map_err(|_|"Application protocol unavailable")?;
    let pid:u32=tokio::time::timeout(Duration::from_secs(2),bus.call("GetConnectionUnixProcessID",&(owner.as_str(),))).await.map_err(|_|"Application protocol timed out")?.map_err(|_|"Application protocol unavailable")?;
    if identity["pid"].as_u64()!=Some(pid as u64){return Err("Application protocol owner does not match the observed native process".into());}
    let path=format!("/{}",app.replace('.',"/").replace('-',"_"));
    Ok((connection,owner,path))
}
fn revalidate(lifecycle:&mut Lifecycle,app:&str,identity:&Value)->Result<()>{
    if lifecycle.exact_process(app)?.identity!=*identity{return Err("Application changed before dispatch".into());}
    Ok(())
}
impl QuitProtocol for Cooperative{
    fn prepare(&self,lifecycle:&mut Lifecycle,app:&str)->Result<Prepared>{
        if !protocol_id(app){return Err("No supported application protocol identity".into());}
        let identity=lifecycle.exact_process(app)?.identity.clone();
        let owner=runtime(async{
            let (connection,owner,path)=endpoint(app,&identity).await?;
            let proxy=zbus::Proxy::new(&connection,owner.as_str(),path.as_str(),"org.gtk.Actions").await.map_err(|_|"Application protocol unavailable")?;
            let actions:Vec<String>=tokio::time::timeout(Duration::from_secs(2),proxy.call("List",&())).await.map_err(|_|"Application protocol timed out")?.map_err(|_|"Application protocol unavailable")?;
            if !actions.iter().any(|action|action=="quit"){return Err("This application does not advertise a cooperative Quit action".into());}
            let description:(bool,Signature,Vec<OwnedValue>)=tokio::time::timeout(Duration::from_secs(2),proxy.call("Describe",&("quit",))).await.map_err(|_|"Application protocol timed out")?.map_err(|_|"Application protocol unavailable")?;
            if !description.0||!description.1.to_string().is_empty(){return Err("The advertised Quit action is unavailable or requires unsupported parameters".into());}
            Ok(owner)
        })?;
        revalidate(lifecycle,app,&identity)?;
        let current_owner=runtime(async{let (_,owner,_)=endpoint(app,&identity).await?;Ok(owner)})?;
        if current_owner!=owner{return Err("Application changed before dispatch".into());}
        Ok(Prepared{app:app.into(),identity,owner})
    }
    fn dispatch(&self,lifecycle:&mut Lifecycle,prepared:&Prepared)->Result<()>{
        revalidate(lifecycle,&prepared.app,&prepared.identity)?;
        runtime(async{
            let (connection,owner,path)=endpoint(&prepared.app,&prepared.identity).await?;
            if owner!=prepared.owner{return Err("Application changed before dispatch".into());}
            let proxy=zbus::Proxy::new(&connection,owner.as_str(),path.as_str(),"org.gtk.Actions").await.map_err(|_|"Application protocol unavailable")?;
            let _:()=tokio::time::timeout(Duration::from_secs(2),proxy.call("Activate",&("quit",Vec::<OwnedValue>::new(),HashMap::<String,OwnedValue>::new()))).await.map_err(|_|"Application protocol timed out")?.map_err(|_|"Application protocol unavailable")?;
            Ok(())
        })
    }
}
impl Cooperative{
    pub fn quit(&self,lifecycle:&mut Lifecycle,app:&str,instance_ids:Option<&Value>)->Result<Value>{
        let prepared=self.prepare(lifecycle,app)?;
        if let Some(expected)=instance_ids{
            let digest=format!("{:x}",Sha256::digest(serde_json::to_vec(&prepared.identity).map_err(|error|error.to_string())?));
            if *expected!=json!([digest]){return Err("The selected application process changed".into());}
        }
        self.dispatch(lifecycle,&prepared)?;
        Ok(json!({"requested":true,"protocol":"org.gtk.Actions","status":"requested-not-confirmed-exited","appId":app}))
    }
}
