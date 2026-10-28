use crate::Result;
use serde_json::Value;
use std::collections::HashSet;

fn integer(value:&Value,zero:bool)->Result<u64>{
    value.as_u64().filter(|number|*number<=i64::MAX as u64&&(zero||*number>0)).ok_or("Invalid native group identity".into())
}
pub fn command(state:&Value,request:&Value)->Result<String>{
    if state["version"].as_u64()!=Some(1){return Err("This compositor does not support native window groups".into());}
    for key in ["enabled","locked","separate_displays"]{if !state[key].is_boolean(){return Err("Invalid native group capability".into());}}
    if state["separate_displays"]!=true{return Err("Window groups require independent desktops on each display".into());}
    let rows=state["groups"].as_array().filter(|rows|rows.len()<=256).ok_or("Invalid native group list")?;
    let mut seen=HashSet::new();let mut members=HashSet::new();let mut selected=HashSet::new();
    for row in rows{
        let id=integer(&row["id"],false)?;let workspace=integer(&row["workspace"],false)?;
        if !seen.insert(id){return Err("Duplicate native group".into());}
        if !row["selected"].is_boolean()||!row["suspended"].is_boolean(){return Err("Invalid native group state".into());}
        if row["selected"]==true&&!selected.insert(workspace){return Err("Conflicting selected groups".into());}
        let items=row["members"].as_array().filter(|items|items.len()<=4096).ok_or("Invalid native member list")?;
        let _=integer(&row["recent"],true)?;
        for item in items{
            let id=integer(&item["id"],false)?;let _=integer(&item["pid"],false)?;
            if !members.insert(id)||!item["group_hidden"].is_boolean(){return Err("Invalid group membership".into());}
        }
    }
    let revision=integer(&state["revision"],true)?;
    if request["groupRevision"].as_u64()!=Some(revision){return Err("Window groups changed; refresh before acting".into());}
    if state["locked"]==true{return Err("Window groups are unavailable while locked".into());}
    let action=request["groupAction"].as_str().ok_or("Unknown window group action")?;
    let prefix=format!("lumen_window groups {revision} ");
    if matches!(action,"on"|"off"){return Ok(prefix+action);}
    if state["enabled"]!=true{return Err("Enable window groups first".into());}
    let gid=integer(&request["groupId"],false)?;
    let group=rows.iter().find(|row|row["id"].as_u64()==Some(gid)).filter(|row|row["suspended"]!=true).ok_or("Group is unavailable in the current presentation")?;
    if action=="select"{return Ok(prefix+&format!("select {gid}"));}
    if !matches!(action,"add"|"remove"){return Err("Unknown window group action".into());}
    let id=integer(&request["id"],false)?;let pid=integer(&request["pid"],false)?;
    if !rows.iter().flat_map(|row|row["members"].as_array().into_iter().flatten()).any(|member|member["id"].as_u64()==Some(id)&&member["pid"].as_u64()==Some(pid)){
        return Err("The captured window is no longer available".into());
    }
    if action=="remove"&&!group["members"].as_array().is_some_and(|items|items.iter().any(|member|member["id"].as_u64()==Some(id)&&member["pid"].as_u64()==Some(pid))){return Err("The window is no longer in that group".into());}
    Ok(prefix+&format!("{action} {gid} {id} {pid}"))
}

#[cfg(test)]
mod tests{
    use super::*;
    use serde_json::json;
    #[test]
    fn captured_group_command_rejects_stale_and_injected_inputs(){
        let state=json!({"version":1,"revision":7,"enabled":true,"locked":false,"separate_displays":true,"groups":[{"id":1,"workspace":10,"selected":true,"recent":2,"suspended":false,"members":[{"id":101,"pid":501,"group_hidden":false}]},{"id":2,"workspace":10,"selected":false,"recent":1,"suspended":false,"members":[{"id":102,"pid":502,"group_hidden":true}]}]});
        let mut request=json!({"groupRevision":7,"groupId":1,"id":102,"pid":502,"groupAction":"add"});
        assert_eq!(command(&state,&request).unwrap(),"lumen_window groups 7 add 1 102 502");
        request["id"]=json!("102; kill");assert!(command(&state,&request).is_err());
        request["id"]=json!(102);request["groupRevision"]=json!(6);assert!(command(&state,&request).is_err());
        request["groupRevision"]=json!(7);request["groupAction"]=json!("remove");assert!(command(&state,&request).is_err());
        request["groupAction"]=json!("off");assert_eq!(command(&state,&request).unwrap(),"lumen_window groups 7 off");
    }
}
