use serde_json::{Value,json};
use std::collections::{BTreeMap,HashMap,HashSet};
use std::fs::{self,File,OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt,OpenOptionsExt};
use std::path::PathBuf;
use uuid::Uuid;
pub mod groups;

pub type Result<T> = std::result::Result<T,String>;
fn string<'a>(value:&'a Value,key:&str)->Result<&'a str>{value[key].as_str().ok_or_else(||format!("Missing {key}"))}
fn number(value:&Value,key:&str)->Result<i64>{value[key].as_i64().ok_or_else(||format!("Missing {key}"))}
fn default_state()->Value{json!({"version":1,"desktops":{},"assignments":{},"counter":0,"auto_reorder":true,"revision":0,"created_counter":0})}
fn new_id()->String{Uuid::new_v4().to_string()}

pub struct Policy{
    pub path:PathBuf,pub state:Value,pub current:HashMap<i64,String>,pub outputs:BTreeMap<String,String>,pub workspaces:Vec<Value>,
    lock:Option<File>,
}
impl Policy{
    pub fn open(path:PathBuf)->Result<Self>{
        let mut policy=Self{path,state:default_state(),current:HashMap::new(),outputs:BTreeMap::new(),workspaces:Vec::new(),lock:None};
        policy.lock()?;policy.reload()?;Ok(policy)
    }
    pub fn begin(&mut self)->Result<()>{self.lock()?;self.reload()}
    pub fn release(&mut self){self.lock=None;}
    fn lock(&mut self)->Result<()>{
        let parent=self.path.parent().ok_or("Invalid desktop state path")?;
        fs::DirBuilder::new().recursive(true).mode(0o700).create(parent).map_err(|e|e.to_string())?;
        let lock=OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC).open(format!("{}.lock",self.path.display())).map_err(|e|e.to_string())?;
        if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX)}!=0{return Err(std::io::Error::last_os_error().to_string());}
        self.lock=Some(lock);Ok(())
    }
    pub fn reload(&mut self)->Result<()>{
        if self.path.exists(){self.state=serde_json::from_slice(&fs::read(&self.path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;}
        if self.state["version"]!=1||!self.state["desktops"].is_object()||!self.state["assignments"].is_object(){return Err("Invalid desktop policy state".into());}
        if self.state.get("revision").is_none(){self.state["revision"]=json!(0);}
        if self.state.get("created_counter").is_none(){self.state["created_counter"]=json!(0);}
        Ok(())
    }
    pub fn save(&mut self)->Result<()>{
        let revision=self.state["revision"].as_i64().unwrap_or(0)+1;self.state["revision"]=json!(revision);
        let parent=self.path.parent().ok_or("Invalid desktop state path")?;
        fs::DirBuilder::new().recursive(true).mode(0o700).create(parent).map_err(|e|e.to_string())?;
        let temporary=parent.join(format!(".spaces-{}",new_id()));
        let result=(||->Result<()>{
            let mut file=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary).map_err(|e|e.to_string())?;
            serde_json::to_writer(&mut file,&self.state).map_err(|e|e.to_string())?;file.flush().map_err(|e|e.to_string())?;file.sync_all().map_err(|e|e.to_string())?;
            fs::rename(&temporary,&self.path).map_err(|e|e.to_string())?;
            File::open(parent).and_then(|dir|dir.sync_all()).map_err(|e|e.to_string())
        })();let _=fs::remove_file(&temporary);result
    }
    pub fn application(node:&Value)->Option<String>{
        node["app_id"].as_str().filter(|v|!v.is_empty()).map(|v|format!("wayland:{v}"))
            .or_else(||node["window_properties"]["class"].as_str().filter(|v|!v.is_empty()).map(|v|format!("x11:{v}")))
    }
    pub fn display(output:&Value)->Result<String>{
        let identity=["make","model","serial"].map(|key|output[key].as_str().unwrap_or(""));
        if identity.iter().all(|value|!value.is_empty())&&!matches!(identity[2],"Unknown"|"0"){
            Ok(format!("display:{}",json!(identity)))
        }else{Ok(format!("connector:{}",string(output,"name")?))}
    }
    pub fn reconcile(&mut self,workspaces:&[Value],outputs:&[Value])->Result<()>{
        let before=self.state.clone();self.outputs.clear();
        for output in outputs{if output["active"].as_bool()!=Some(false){self.outputs.insert(Self::display(output)?,string(output,"name")?.into());}}
        let focused=outputs.iter().find(|output|output["active"].as_bool()!=Some(false)&&output["focused"]==true).and_then(|o|o["name"].as_str()).map(str::to_owned).or_else(||self.outputs.values().next().cloned());
        let previous=std::mem::take(&mut self.current);
        for workspace in workspaces{
            let name=string(workspace,"name")?;if name=="__i3_scratch"{continue;}
            let id=number(workspace,"id")?;let output=string(workspace,"output")?;
            let key=previous.get(&id).cloned().or_else(||self.state["desktops"].as_object().and_then(|items|items.iter().find(|(_,item)|item["name"]==name).map(|(key,_)|key.clone()))).unwrap_or_else(||{
                let key=new_id();let ordinal=self.state["desktops"].as_object().into_iter().flat_map(|items|items.values()).filter_map(|item|item["ordinal"].as_i64()).max().unwrap_or(-1)+1;
                self.state["desktops"][&key]=json!({"ordinal":ordinal,"recent":0});key
            });
            self.state["desktops"][&key]["name"]=json!(name);self.state["desktops"][&key]["output"]=json!(output);
            self.state["desktops"][&key]["output_id"]=self.outputs.iter().find(|(_,name)|name.as_str()==output).map(|(id,_)|json!(id)).unwrap_or(Value::Null);
            self.current.insert(id,key);
        }
        self.workspaces=workspaces.to_vec();
        let live=self.current.values().cloned().collect::<HashSet<_>>();
        if let Some(items)=self.state["desktops"].as_object_mut(){for (key,item) in items{
            if item["persistent"]!=true||live.contains(key){continue;}
            let old=item["output_id"].as_str().unwrap_or("");
            if let Some(connector)=self.outputs.get(old){item["output"]=json!(connector);}
            else if let Some(connector)=&focused{item["output"]=json!(connector);item["output_id"]=self.outputs.iter().find(|(_,name)|*name==connector).map(|(id,_)|json!(id)).unwrap_or(Value::Null);}
        }}
        if self.state!=before{self.save()?;}Ok(())
    }
    pub fn focus(&mut self,id:i64)->Result<()>{if let Some(key)=self.current.get(&id){
        let counter=self.state["counter"].as_i64().unwrap_or(0)+1;self.state["counter"]=json!(counter);self.state["desktops"][key]["recent"]=json!(counter);self.save()?;
    }Ok(())}
    pub fn order(&self,output:Option<&str>)->Vec<String>{
        let live=self.current.values().collect::<HashSet<_>>();
        let mut keys=self.state["desktops"].as_object().into_iter().flat_map(|items|items.iter()).filter(|(key,item)|{
            (live.contains(key)||item["persistent"]==true)&&output.is_none_or(|o|item["output"]==o)
        }).map(|(key,_)|key.clone()).collect::<Vec<_>>();
        let recent=self.state["auto_reorder"]==true;
        keys.sort_by_key(|key|{let item=&self.state["desktops"][key];(if recent{-item["recent"].as_i64().unwrap_or(0)}else{item["ordinal"].as_i64().unwrap_or(0)},item["ordinal"].as_i64().unwrap_or(0))});keys
    }
    pub fn require_revision(&self,revision:i64)->Result<()>{if self.state["revision"].as_i64()!=Some(revision){Err("Desktop list changed; refresh and try again".into())}else{Ok(())}}
    pub fn catalog(&self)->Vec<Value>{
        let live=self.current.iter().map(|(id,key)|(key.clone(),*id)).collect::<HashMap<_,_>>();
        self.order(None).iter().map(|key|{let item=&self.state["desktops"][key];let id=live.get(key).copied();let ws=id.and_then(|id|self.workspaces.iter().find(|ws|ws["id"]==id));
            json!({"logicalId":key,"name":item["name"],"label":item["label"].as_str().map(str::to_owned).unwrap_or_else(||format!("Desktop {}",item["name"].as_str().unwrap_or(""))),"output":item["output"],"outputId":item["output_id"],"available":self.outputs.values().any(|name|item["output"]==*name),"persistent":item["persistent"]==true,"id":id,"focused":ws.is_some_and(|ws|ws["focused"]==true),"visible":ws.is_some_and(|ws|ws["visible"]==true)})
        }).collect()
    }
    pub fn desktop(&self,key:&str,connected:bool)->Result<Value>{
        let item=self.catalog().into_iter().find(|item|item["logicalId"]==key).ok_or("This desktop is no longer available")?;
        if connected&&item["available"]!=true{return Err("The desktop display is disconnected".into());}Ok(item)
    }
    pub fn create(&mut self,output_id:&str,revision:i64)->Result<Value>{
        self.require_revision(revision)?;let connector=self.outputs.get(output_id).cloned().ok_or("The desktop display is disconnected")?;
        if self.catalog().len()>=64{return Err("The desktop limit is 64".into());}
        let key=new_id();let count=self.state["created_counter"].as_i64().unwrap_or(0)+1;self.state["created_counter"]=json!(count);
        let ordinal=self.state["desktops"].as_object().into_iter().flat_map(|items|items.values()).filter_map(|item|item["ordinal"].as_i64()).max().unwrap_or(-1)+1;
        self.state["desktops"][&key]=json!({"name":format!("lumen:{key}"),"label":format!("Desktop {count}"),"persistent":true,"output":connector,"output_id":output_id,"ordinal":ordinal,"recent":0});
        self.save()?;self.desktop(&key,true)
    }
    pub fn reorder(&mut self,output_id:&str,order:&[String],revision:i64)->Result<()>{
        self.require_revision(revision)?;let connector=self.outputs.get(output_id).ok_or("The desktop display is disconnected")?;
        let current=self.order(Some(connector));
        if order.len()!=current.len()||order.iter().collect::<HashSet<_>>()!=current.iter().collect::<HashSet<_>>(){return Err("Manual order must contain every desktop on this display exactly once".into());}
        let mut slots=current.iter().map(|key|self.state["desktops"][key]["ordinal"].as_i64().unwrap_or(0)).collect::<Vec<_>>();slots.sort();
        for (key,slot) in order.iter().zip(slots){self.state["desktops"][key]["ordinal"]=json!(slot);}
        self.state["auto_reorder"]=json!(false);self.save()
    }
    pub fn remove(&mut self,source:&str,destination:&str,revision:i64)->Result<()>{
        self.require_revision(revision)?;let old=self.state["desktops"][source].clone();let target=self.desktop(destination,true)?;
        if old.is_null(){return Err("This desktop is no longer available".into());}
        if source==destination||old["output_id"]!=target["outputId"]{return Err("Choose a different surviving desktop on the same display".into());}
        if self.current.values().any(|key|key==source){return Err("The desktop still exists in the compositor".into());}
        self.state["desktops"].as_object_mut().ok_or("Invalid desktop state")?.remove(source);self.save()
    }
    pub fn configure(&mut self,auto:bool)->Result<()>{self.state["auto_reorder"]=json!(auto);self.save()}
    pub fn assign(&mut self,app:&str,kind:&str,target:Option<&str>)->Result<()>{
        if !(app.starts_with("wayland:")||app.starts_with("x11:"))||app.split_once(':').is_none_or(|(_,name)|name.is_empty()){return Err("An exact typed application identity is required".into());}
        if !matches!(kind,"none"|"all"|"desktop"|"display")||matches!(kind,"none"|"all")!=target.is_none()||target==Some(""){return Err("Invalid application assignment".into());}
        self.state["assignments"][app]=json!({"kind":kind,"target_id":target});self.save()
    }
    pub fn destination(&self,node:&Value)->Value{
        let app=Self::application(node).unwrap_or_default();let assignment=self.state["assignments"][&app].as_object().map(|_|self.state["assignments"][&app].clone()).unwrap_or(json!({"kind":"none","target_id":null}));
        let kind=assignment["kind"].as_str().unwrap_or("none");let target=assignment["target_id"].as_str().unwrap_or("");
        if kind=="none"{return json!({"status":"unassigned"});}
        if kind=="all"{return json!({"status":"membership","scope":"current-display","assignment":assignment});}
        if kind=="desktop"{let item=&self.state["desktops"][target];if !item.is_null(){
            if self.current.values().any(|key|key==target){return json!({"status":"available","workspace":item["name"]});}
            if self.outputs.values().any(|name|item["output"]==*name){return json!({"status":"available","workspace":item["name"],"recreate_on_output":item["output"]});}
        }}else if kind=="display"{if let Some(connector)=self.outputs.get(target){if let Some(ws)=self.workspaces.iter().find(|ws|ws["output"]==*connector&&ws["visible"]==true){return json!({"status":"available","workspace":ws["name"]});}}}
        json!({"status":"unavailable","reason":"Assignment target is unavailable","fallback":"keep-current-placement","assignment":assignment})
    }
    pub fn status(&self)->Value{json!({"desktops":self.state["desktops"],"order":self.order(None),"assignments":self.state["assignments"],"auto_reorder":self.state["auto_reorder"],"capabilities":{"all_desktops":{"supported":true,"scope":"current-display","window_kind":"independent-floating"}},"outputs":self.outputs})}
}

pub fn windows(tree:&Value)->Vec<Value>{
    let grouped=tree["window_groups"]["groups"].as_array().into_iter().flatten().flat_map(|group|group["members"].as_array().into_iter().flatten()).filter_map(|member|member["id"].as_u64()).collect::<HashSet<_>>();
    fn walk(node:&Value,workspace:&str,output:&str,floating:Option<u64>,groups:&HashSet<u64>,found:&mut Vec<Value>){
        let workspace=if node["type"]=="workspace"{node["name"].as_str().unwrap_or(workspace)}else{workspace};
        let output=if node["type"]=="output"{node["name"].as_str().unwrap_or(output)}else{output};
        if node["pid"].as_u64().is_some_and(|pid|pid>0)&&(node["app_id"].is_string()||!node["window"].is_null()){
            let mut item=node.clone();item["_workspace"]=json!(workspace);item["_output"]=json!(output);item["_floating"]=json!(floating==node["id"].as_u64());item["_grouped"]=json!(node["id"].as_u64().is_some_and(|id|groups.contains(&id)));found.push(item);
        }
        if let Some(children)=node["nodes"].as_array(){for child in children{walk(child,workspace,output,floating,groups,found);}}
        if let Some(children)=node["floating_nodes"].as_array(){for child in children{walk(child,workspace,output,child["id"].as_u64(),groups,found);}}
    }
    let mut found=Vec::new();walk(tree,"","",None,&grouped,&mut found);found
}
pub fn membership_reason(node:&Value)->Option<&'static str>{
    let relation=&node["lumen_relationships"];let state=&node["lumen_state"];
    if relation["parent_supported"]!=true{return Some("Window relationships are unavailable for this application");}
    if !relation["parent"].is_null()||relation["modal"]==true{return Some("Owned and modal windows retain their parent placement");}
    if node["_grouped"]==true{return Some("Turn off window groups before assigning all desktops");}
    if node["_floating"]!=true{return Some("Tiled windows retain their layout; float the window to use all desktops");}
    let pair=&node["lumen_pair"];
    if node["fullscreen_mode"].as_i64().unwrap_or(0)!=0||!pair["id"].is_null()||pair["survivor"]==true{return Some("Full screen and paired windows retain their saved placement");}
    if node["_workspace"]=="__i3_scratch"||["minimised","filled","hidden","desktop_hidden"].iter().any(|key|state[*key]==true){return Some("Restore this window before assigning all desktops");}
    None
}
pub fn process_instance(node:&Value,epoch:&str)->Option<Value>{
    let pid=node["pid"].as_u64().filter(|pid|*pid>0)?;
    let stat=fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let start=stat.rsplit_once(')')?.1.split_whitespace().nth(19)?;
    Some(json!({"epoch":epoch,"id":node["id"],"pid":pid,"start":start}))
}
pub fn membership(policy:&Policy,node:&Value,epoch:&str)->Value{
    let app=Policy::application(node).unwrap_or_default();
    if policy.state["assignments"][&app]["kind"]!="all"{return json!({"status":"unassigned"});}
    if let Some(reason)=membership_reason(node){return json!({"status":"pending","reason":reason});}
    if process_instance(node,epoch).is_none(){return json!({"status":"pending","reason":"The application process instance is unavailable"});}
    if node["sticky"]==true{json!({"status":"active","reason":"Every desktop on this window’s current display"})}else{json!({"status":"pending","reason":"Waiting for native membership"})}
}
pub fn reconcile_membership(policy:&mut Policy,tree:&Value,epoch:&str,mut command:impl FnMut(&str)->Result<()>)->Result<Value>{
    let before=policy.state["managed_sticky"].as_object().cloned().unwrap_or_default();let mut managed=before.clone();let observed=windows(tree);
    let live=observed.iter().filter_map(|node|node["id"].as_u64().map(|id|(id.to_string(),node))).collect::<HashMap<_,_>>();
    for (key,record) in &before{
        let Some(node)=live.get(key)else{managed.remove(key);continue;};
        if process_instance(node,epoch).as_ref()!=Some(record){managed.remove(key);continue;}
        let app=Policy::application(node).unwrap_or_default();
        if policy.state["assignments"][&app]["kind"]!="all"||membership_reason(node).is_some(){
            if node["sticky"]==true{command(&format!("[con_id={key}] sticky disable"))?;}managed.remove(key);
        }
    }
    for node in &observed{
        let app=Policy::application(node).unwrap_or_default();
        if policy.state["assignments"][&app]["kind"]!="all"||membership_reason(node).is_some(){continue;}
        let Some(identity)=process_instance(node,epoch)else{continue;};if node["sticky"]==true{continue;}
        let key=node["id"].as_u64().ok_or("Invalid window identity")?.to_string();managed.insert(key.clone(),identity);
        policy.state["managed_sticky"]=json!(managed);policy.save()?;
        command(&format!("[con_id={key}] sticky enable"))?;
    }
    if managed!=before{policy.state["managed_sticky"]=json!(managed);policy.save()?;}
    Ok(Value::Object(observed.iter().filter_map(|node|node["id"].as_u64().map(|id|(id.to_string(),membership(policy,node,epoch)))).collect()))
}

#[cfg(test)]
mod tests{
    use super::*;
    fn fixture()->(PathBuf,Policy,Vec<Value>,Vec<Value>){
        let root=std::env::temp_dir().join(format!("lumen-spaces-test-{}",new_id()));
        let mut policy=Policy::open(root.join("spaces.json")).unwrap();
        let outputs=vec![json!({"name":"A","make":"Acme","model":"Panel","serial":"123","active":true})];
        let workspaces=(1..=3).map(|id|json!({"id":id,"name":id.to_string(),"output":"A","visible":id==1})).collect::<Vec<_>>();
        policy.reconcile(&workspaces,&outputs).unwrap();(root,policy,workspaces,outputs)
    }
    #[test]
    fn identity_order_and_typed_assignment_survive_restart(){
        let (root,mut policy,mut workspaces,outputs)=fixture();
        let key=policy.current[&1].clone();workspaces[0]["name"]=json!("Work");policy.reconcile(&workspaces,&outputs).unwrap();assert_eq!(policy.current[&1],key);
        let initial=policy.current.clone();policy.focus(2).unwrap();policy.focus(3).unwrap();assert_eq!(policy.order(None),vec![initial[&3].clone(),initial[&2].clone(),initial[&1].clone()]);
        policy.configure(false).unwrap();assert_eq!(policy.order(None),vec![initial[&1].clone(),initial[&2].clone(),initial[&3].clone()]);
        policy.assign("wayland:editor","desktop",Some(&initial[&2])).unwrap();
        assert_eq!(policy.destination(&json!({"app_id":"editor"}))["workspace"],"2");
        assert_eq!(policy.destination(&json!({"window_properties":{"class":"editor"}}))["status"],"unassigned");
        policy.release();let mut restarted=Policy::open(root.join("spaces.json")).unwrap();workspaces[0]["id"]=json!(90);restarted.reconcile(&workspaces,&outputs).unwrap();
        assert_eq!(restarted.current[&90],key);assert_eq!(restarted.destination(&json!({"app_id":"editor"}))["workspace"],"2");
        drop(restarted);drop(policy);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unavailable_assignment_and_revision_fail_closed(){
        let (root,mut policy,workspaces,outputs)=fixture();
        policy.assign("wayland:editor","desktop",Some("missing")).unwrap();assert_eq!(policy.destination(&json!({"app_id":"editor"}))["status"],"unavailable");
        policy.assign("wayland:editor","all",None).unwrap();assert_eq!(policy.destination(&json!({"app_id":"editor"}))["scope"],"current-display");
        assert!(policy.assign("wayland:editor","all",Some("target")).is_err());
        let revision=policy.state["revision"].as_i64().unwrap();assert!(policy.create("connector:missing",revision).is_err());assert!(policy.create("connector:missing",revision-1).is_err());
        let output=Policy::display(&outputs[0]).unwrap();policy.assign("wayland:editor","display",Some(&output)).unwrap();assert_eq!(policy.destination(&json!({"app_id":"editor"}))["workspace"],"1");
        policy.reconcile(&[],&[]).unwrap();assert_eq!(policy.destination(&json!({"app_id":"editor"}))["fallback"],"keep-current-placement");
        policy.reconcile(&workspaces,&outputs).unwrap();assert_eq!(policy.destination(&json!({"app_id":"editor"}))["workspace"],"1");
        drop(policy);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn membership_changes_only_owned_sticky_windows(){
        let (root,mut policy,_,_)=fixture();
        policy.assign("wayland:editor","all",None).unwrap();
        let node=json!({"id":42,"pid":std::process::id(),"app_id":"editor","sticky":false,"lumen_relationships":{"parent_supported":true,"parent":null,"modal":null}});
        let tree=json!({"nodes":[{"type":"workspace","name":"1","floating_nodes":[node]}]});
        let mut calls=Vec::new();reconcile_membership(&mut policy,&tree,"session",|cmd|{calls.push(cmd.to_owned());Ok(())}).unwrap();
        assert_eq!(calls,["[con_id=42] sticky enable"]);
        let mut sticky=tree.clone();sticky["nodes"][0]["floating_nodes"][0]["sticky"]=json!(true);
        calls.clear();reconcile_membership(&mut policy,&sticky,"session",|cmd|{calls.push(cmd.to_owned());Ok(())}).unwrap();assert!(calls.is_empty());
        policy.assign("wayland:editor","none",None).unwrap();
        reconcile_membership(&mut policy,&sticky,"session",|cmd|{calls.push(cmd.to_owned());Ok(())}).unwrap();assert_eq!(calls,["[con_id=42] sticky disable"]);
        policy.assign("wayland:editor","all",None).unwrap();calls.clear();
        reconcile_membership(&mut policy,&sticky,"session",|cmd|{calls.push(cmd.to_owned());Ok(())}).unwrap();
        policy.assign("wayland:editor","none",None).unwrap();reconcile_membership(&mut policy,&sticky,"replacement",|cmd|{calls.push(cmd.to_owned());Ok(())}).unwrap();assert!(calls.is_empty());
        drop(policy);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unsupported_membership_and_grouped_windows_remain_untouched(){
        let (root,policy,_,_)=fixture();
        for change in [json!({"_floating":false}),json!({"_grouped":true}),json!({"fullscreen_mode":1}),json!({"lumen_pair":{"id":4}}),json!({"lumen_relationships":{"parent_supported":false}}),json!({"lumen_relationships":{"parent_supported":true,"parent":9}}),json!({"lumen_relationships":{"parent_supported":true,"modal":true}}),json!({"lumen_state":{"filled":true}})]{
            let mut node=json!({"_floating":true,"_grouped":false,"lumen_relationships":{"parent_supported":true,"parent":null,"modal":null}});
            for (key,value) in change.as_object().unwrap(){node[key]=value.clone();}
            assert!(membership_reason(&node).is_some(),"{node}");
        }
        let tree=json!({"window_groups":{"groups":[{"members":[{"id":42}]}]},"nodes":[{"type":"workspace","name":"1","floating_nodes":[{"id":42,"pid":12,"app_id":"editor"}]}]});
        let node=&windows(&tree)[0];assert_eq!(node["_floating"],true);assert_eq!(node["_grouped"],true);assert_eq!(node["_workspace"],"1");
        drop(policy);fs::remove_dir_all(root).unwrap();
    }
}
