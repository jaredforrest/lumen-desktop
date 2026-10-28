use crate::address::{address, PROTOCOLS};
use gio::prelude::*;
use glib::translate::{from_glib_full, ToGlibPtr};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::rc::Rc;

type Result<T> = std::result::Result<T, &'static str>;
type Shared = Rc<RefCell<Backend>>;

#[link(name = "gtk-3")]
unsafe extern "C" {
    fn gtk_init_check(argc: *mut libc::c_int, argv: *mut *mut *mut libc::c_char) -> libc::c_int;
    fn gtk_mount_operation_new(parent: *mut libc::c_void) -> *mut gio::ffi::GMountOperation;
}

struct Connection {
    id: String, uri: String, name: String, state: &'static str,
    path: String, error: String, root: gio::File, mount: Option<gio::Mount>,
    can_disconnect: bool, generation: u64,
}
impl Connection {
    fn public(&self) -> Value {
        json!({"id":self.id,"uri":self.uri,"name":self.name,"state":self.state,
            "path":self.path,"canDisconnect":self.can_disconnect,"error":self.error})
    }
}

struct Operation {
    action: &'static str, connection: String, generation: u64,
    cancellable: gio::Cancellable, toolkit: Option<gio::MountOperation>, timer: Option<glib::SourceId>,
    enumerator: Option<gio::FileEnumerator>, target: Option<gio::File>,
    rows: Vec<Value>, scanned: usize,
}

struct Backend {
    monitor: gio::VolumeMonitor, vfs: gio::Vfs,
    connections: Vec<Connection>, operations: HashMap<i64, Operation>, closed: bool,
    anonymous_fixture: bool,
}
impl Backend {
    fn new(anonymous_fixture: bool) -> Self {
        Self { monitor:gio::VolumeMonitor::get(),vfs:gio::Vfs::default(),
            connections:Vec::new(),operations:HashMap::new(),closed:false,anonymous_fixture }
    }
    fn protocols(&self) -> Vec<&'static str> {
        let available=self.vfs.supported_uri_schemes();
        PROTOCOLS.iter().copied().filter(|scheme|available.iter().any(|value|value.as_str()==*scheme)).collect()
    }
    fn snapshot(&self) -> Value {
        let protocols=self.protocols();
        json!({"connections":self.connections.iter().map(Connection::public).collect::<Vec<_>>(),
            "protocols":protocols,"available":!protocols.is_empty(),
            "reason":if protocols.is_empty(){"Network providers are unavailable; enable the optional GVfs host integration"}else{""}})
    }
    fn connection(&self, id:&str) -> Option<&Connection> { self.connections.iter().find(|entry|entry.id==id) }
    fn connection_mut(&mut self, id:&str) -> Option<&mut Connection> { self.connections.iter_mut().find(|entry|entry.id==id) }
    fn alive(&self, id:i64) -> bool {
        self.operations.get(&id).and_then(|op|self.connection(&op.connection).map(|entry|entry.generation==op.generation)).unwrap_or(false)
    }
}

fn emit(value:Value) {println!("{value}");let _=std::io::stdout().flush();}
fn response(id:i64,action:&str,mut value:Value) {value["id"]=json!(id);value["action"]=json!(action);emit(value);}
fn changed(shared:&Shared) {
    let backend=shared.borrow();
    if !backend.closed {let mut value=backend.snapshot();value["event"]=json!("state");emit(value);}
}
fn new_id() -> Option<String> {Some(std::fs::read_to_string("/proc/sys/kernel/random/uuid").ok()?.trim().replace('-',""))}
fn host(uri:&str) -> String {
    glib::Uri::parse(uri,glib::UriFlags::NONE).ok().and_then(|value|value.host().map(|host|host.to_string())).unwrap_or_default()
}
fn reason(error:&glib::Error) -> &'static str {
    match error.kind::<gio::IOErrorEnum>() {
        Some(gio::IOErrorEnum::Cancelled)=>"The operation was cancelled",
        Some(gio::IOErrorEnum::NotSupported)=>"This network provider is not available in the current session",
        Some(gio::IOErrorEnum::NotMounted)=>"The network location is disconnected",
        Some(gio::IOErrorEnum::HostNotFound)=>"The server could not be found",
        Some(gio::IOErrorEnum::ConnectionRefused)=>"The server refused the connection",
        Some(gio::IOErrorEnum::PermissionDenied)=>"The server denied access; check the account in the system dialog",
        Some(gio::IOErrorEnum::TimedOut)=>"The network request timed out",
        Some(gio::IOErrorEnum::NotFound)=>"This network folder is no longer available",
        _=>"The network provider could not complete the operation",
    }
}
fn operation(anonymous_fixture:bool) -> Result<gio::MountOperation> {
    if anonymous_fixture {return Ok(gio::MountOperation::new());}
    unsafe {
        if gtk_init_check(std::ptr::null_mut(),std::ptr::null_mut())==0 {return Err("A graphical session is required for network authentication");}
        let value=gtk_mount_operation_new(std::ptr::null_mut());
        if value.is_null(){return Err("A graphical session is required for network authentication");}
        Ok(from_glib_full(value))
    }
}

fn refresh_mounts(shared:&Shared, notify:bool) {
    let mounts=shared.borrow().monitor.mounts();
    let mut present=HashSet::new();
    for mount in mounts {
        let Ok(uri)=address(&mount.root().uri(),true) else {continue};
        let root=shared.borrow().vfs.file_for_uri(&uri);
        let existing={let backend=shared.borrow();backend.connections.iter().find(|entry|entry.mount.as_ref()==Some(&mount))
            .or_else(||backend.connections.iter().find(|entry|matches!(entry.state,"connecting"|"authenticating"|"disconnected")
                && (entry.root.equal(&root)||entry.root.has_prefix(&root)))).map(|entry|entry.id.clone())};
        let id=match existing {Some(id)=>id,None=>{
            let Some(id)=new_id() else {continue};
            shared.borrow_mut().connections.push(Connection{id:id.clone(),uri:uri.clone(),name:host(&uri),state:"connected",
                path:String::new(),error:String::new(),root:root.clone(),mount:None,can_disconnect:false,generation:0});id
        }};
        present.insert(id.clone());
        let mut backend=shared.borrow_mut();let entry=backend.connection_mut(&id).unwrap();entry.mount=Some(mount.clone());
        if !matches!(entry.state,"connecting"|"authenticating"|"disconnecting") {
            entry.state="connected";entry.path=entry.root.path().map(|path|path.to_string_lossy().into_owned()).unwrap_or_default();
            entry.can_disconnect=mount.can_unmount();entry.error.clear();
        }
    }
    let mut retired=Vec::new();
    {
        let mut backend=shared.borrow_mut();
        for entry in &mut backend.connections {
            if entry.mount.is_some() && !present.contains(&entry.id) && entry.state!="disconnecting" {
                entry.state="disconnected";entry.mount=None;entry.path.clear();entry.can_disconnect=false;
                entry.error="The network location disconnected".into();entry.generation+=1;retired.push(entry.id.clone());
            }
        }
    }
    for connection in retired {
        let ids={let backend=shared.borrow();backend.operations.iter().filter(|(_,op)|op.connection==connection).map(|(id,_)|*id).collect::<Vec<_>>()};
        for id in ids {cancel(shared,id,"The network location disconnected");}
    }
    if notify {changed(shared);}
}

fn release(shared:&Shared,id:i64) -> Option<Operation> {
    let mut op=shared.borrow_mut().operations.remove(&id)?;
    if let Some(timer)=op.timer.take(){timer.remove();}
    if let Some(enumerator)=&op.enumerator {enumerator.close_async(glib::Priority::DEFAULT,gio::Cancellable::NONE,|_|{});}
    Some(op)
}
fn cancel(shared:&Shared,id:i64,message:&str) {
    let Some(op)=release(shared,id) else {return};
    op.cancellable.cancel();
    if let Some(toolkit)=op.toolkit {toolkit.emit_by_name::<()>("aborted",&[]);}
    {
        let mut backend=shared.borrow_mut();
        if let Some(entry)=backend.connection_mut(&op.connection) {
            if matches!(op.action,"connect"|"reconnect"|"disconnect") {
                entry.state=if op.action=="disconnect" {"connected"} else {"disconnected"};entry.error=message.into();
                if op.action!="disconnect" {entry.path.clear();}
            }
        }
    }
    response(id,op.action,json!({"connectionId":op.connection,"error":message,"cancelled":true}));changed(shared);
}
fn finish(shared:&Shared,id:i64,result:std::result::Result<Value,glib::Error>) {
    if !shared.borrow().alive(id){return;}
    let Some(op)=release(shared,id) else {return};
    let value=match result {
        Ok(mut value)=>{value["connectionId"]=json!(op.connection);value},
        Err(error)=>{
            let message=reason(&error);
            if matches!(op.action,"connect"|"reconnect"|"disconnect") {
                if let Some(entry)=shared.borrow_mut().connection_mut(&op.connection) {
                    entry.state=if op.action=="disconnect" {"connected"} else {"disconnected"};entry.error=message.into();
                    if op.action!="disconnect" {entry.path.clear();}
                }
            }
            json!({"connectionId":op.connection,"error":message})
        }
    };
    response(id,op.action,value);changed(shared);
}
fn begin(shared:&Shared,id:i64,action:&'static str,connection:&str,seconds:u32,toolkit:bool)->Result<gio::Cancellable> {
    let (generation,anonymous_fixture)={let backend=shared.borrow();(backend.connection(connection).ok_or("This network location is no longer known")?.generation,backend.anonymous_fixture)};
    let mount=if toolkit {Some(operation(anonymous_fixture)?)} else {None};
    let weak=Rc::downgrade(shared);
    let timer=glib::timeout_add_seconds_local(seconds,move||{if let Some(shared)=weak.upgrade(){cancel(&shared,id,"The network request timed out; retry or check the connection");}glib::ControlFlow::Break});
    if let Some(mount)=&mount {
        let weak=Rc::downgrade(shared);
        mount.connect_ask_password(move|_,_,_,_,_|{if let Some(shared)=weak.upgrade(){authenticating(&shared,id);}});
        // GTK owns the question dialog and its answers; this signal only changes public state.
        let weak=Rc::downgrade(shared);
        mount.connect_local("ask-question",false,move|_|{if let Some(shared)=weak.upgrade(){authenticating(&shared,id);}None});
    }
    let cancellable=gio::Cancellable::new();
    shared.borrow_mut().operations.insert(id,Operation{action,connection:connection.into(),generation,cancellable:cancellable.clone(),
        toolkit:mount,timer:Some(timer),enumerator:None,target:None,rows:Vec::new(),scanned:0});
    Ok(cancellable)
}
fn authenticating(shared:&Shared,id:i64) {
    let connection={let backend=shared.borrow();if !backend.alive(id){return;}backend.operations[&id].connection.clone()};
    if let Some(entry)=shared.borrow_mut().connection_mut(&connection){entry.state="authenticating";}
    changed(shared);
}

struct MountCallback(Box<dyn FnOnce(std::result::Result<gio::Mount,glib::Error>)>);
fn find_mount_async(file:&gio::File,cancellable:&gio::Cancellable,
    callback:impl FnOnce(std::result::Result<gio::Mount,glib::Error>)+'static) {
    unsafe extern "C" fn done(source:*mut glib::gobject_ffi::GObject,
        result:*mut gio::ffi::GAsyncResult,data:*mut libc::c_void) {
        let callback=unsafe {Box::from_raw(data.cast::<MountCallback>())};
        let mut error=std::ptr::null_mut();
        let mount=unsafe {gio::ffi::g_file_find_enclosing_mount_finish(source.cast(),result,&mut error)};
        let outcome=if error.is_null() {Ok(unsafe{from_glib_full(mount)})} else {Err(unsafe{from_glib_full(error)})};
        (callback.0)(outcome);
    }
    let callback=Box::new(MountCallback(Box::new(callback)));
    unsafe {gio::ffi::g_file_find_enclosing_mount_async(file.to_glib_none().0,0,
        cancellable.to_glib_none().0,Some(done),Box::into_raw(callback).cast());}
}

fn mounted(shared:&Shared,id:i64,result:std::result::Result<(),glib::Error>) {
    if !shared.borrow().alive(id){return;}
    if let Err(error)=result {
        if error.kind::<gio::IOErrorEnum>()!=Some(gio::IOErrorEnum::AlreadyMounted) {finish(shared,id,Err(error));return;}
    }
    let (target,cancellable)={let backend=shared.borrow();let op=&backend.operations[&id];
        (backend.connection(&op.connection).unwrap().root.clone(),op.cancellable.clone())};
    let weak=Rc::downgrade(shared);
    find_mount_async(&target,&cancellable,move|result|{
        if let Some(shared)=weak.upgrade(){located(&shared,id,result);}
    });
}
fn located(shared:&Shared,id:i64,result:std::result::Result<gio::Mount,glib::Error>) {
    if !shared.borrow().alive(id){return;}
    let mount=match result {Ok(mount)=>mount,Err(error)=>{finish(shared,id,Err(error));return;}};
    let (connection,path,uri)={let backend=shared.borrow();let op=&backend.operations[&id];let entry=backend.connection(&op.connection).unwrap();
        (entry.id.clone(),entry.root.path().map(|path|path.to_string_lossy().into_owned()).unwrap_or_default(),entry.uri.clone())};
    {let mut backend=shared.borrow_mut();let entry=backend.connection_mut(&connection).unwrap();
        entry.mount=Some(mount.clone());entry.state="connected";entry.path=path.clone();entry.can_disconnect=mount.can_unmount();entry.error.clear();}
    finish(shared,id,Ok(json!({"connected":true,"path":path,"uri":uri})));
}

fn disconnect(shared:&Shared,id:i64,connection:&str)->Result<()> {
    let mount={let backend=shared.borrow();let entry=backend.connection(connection).ok_or("This network location is no longer known")?;
        if entry.state!="connected" || !entry.can_disconnect {return Err("This network location cannot be disconnected");}
        entry.mount.clone().ok_or("This network location cannot be disconnected")?};
    let cancellable=begin(shared,id,"disconnect",connection,30,true)?;
    {let mut backend=shared.borrow_mut();let entry=backend.connection_mut(connection).unwrap();entry.state="disconnecting";entry.error.clear();}
    changed(shared);
    let toolkit=shared.borrow().operations[&id].toolkit.clone().unwrap();
    let weak=Rc::downgrade(shared);
    mount.unmount_with_operation(gio::MountUnmountFlags::NONE,Some(&toolkit),Some(&cancellable),move|result|{
        let Some(shared)=weak.upgrade() else {return};
        if !shared.borrow().alive(id){return;}
        match result {
            Ok(())=>{
                let connection=shared.borrow().operations[&id].connection.clone();
                let generation={let mut backend=shared.borrow_mut();let entry=backend.connection_mut(&connection).unwrap();
                    entry.state="disconnected";entry.mount=None;entry.path.clear();entry.can_disconnect=false;entry.error.clear();entry.generation+=1;entry.generation};
                shared.borrow_mut().operations.get_mut(&id).unwrap().generation=generation;
                finish(&shared,id,Ok(json!({"disconnected":true})));
            }
            Err(error)=>finish(&shared,id,Err(error)),
        }
    });
    Ok(())
}

fn browse(shared:&Shared,id:i64,connection:&str,uri:Option<&str>)->Result<()> {
    let (root,target)={let backend=shared.borrow();let entry=backend.connection(connection).ok_or("This network location is no longer known")?;
        if entry.state!="connected"{return Err("Connect this network location before browsing");}
        let uri=address(uri.unwrap_or(&entry.uri),false).map_err(|_|"Enter a valid server address")?;
        (entry.root.clone(),backend.vfs.file_for_uri(&uri))};
    if !target.equal(&root) && !target.has_prefix(&root){return Err("Choose a folder inside this network location");}
    let retired={let backend=shared.borrow();backend.operations.iter().filter(|(_,op)|op.connection==connection && op.action=="browse")
        .map(|(id,_)|*id).collect::<Vec<_>>()};
    for old in retired {cancel(shared,old,"A newer folder request replaced this request");}
    let cancellable=begin(shared,id,"browse",connection,8,false)?;
    shared.borrow_mut().operations.get_mut(&id).unwrap().target=Some(target.clone());
    let weak=Rc::downgrade(shared);
    target.enumerate_children_async("standard::name,standard::display-name,standard::type,standard::size,time::modified",
        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,glib::Priority::DEFAULT,Some(&cancellable),move|result|{
            let Some(shared)=weak.upgrade() else {return};
            if !shared.borrow().alive(id){return;}
            match result {
                Ok(enumerator)=>{shared.borrow_mut().operations.get_mut(&id).unwrap().enumerator=Some(enumerator);next_batch(&shared,id);}
                Err(error)=>finish(&shared,id,Err(error)),
            }
        });
    Ok(())
}
fn next_batch(shared:&Shared,id:i64) {
    if !shared.borrow().alive(id){return;}
    let (enumerator,cancellable)={let backend=shared.borrow();let op=&backend.operations[&id];(op.enumerator.clone().unwrap(),op.cancellable.clone())};
    let weak=Rc::downgrade(shared);
    enumerator.next_files_async(32,glib::Priority::DEFAULT,Some(&cancellable),move|result|{
        let Some(shared)=weak.upgrade() else {return};
        if !shared.borrow().alive(id){return;}
        match result {Ok(infos)=>received_batch(&shared,id,infos),Err(error)=>finish(&shared,id,Err(error))}
    });
}
fn received_batch(shared:&Shared,id:i64,infos:Vec<gio::FileInfo>) {
    let (target,root)={let backend=shared.borrow();let op=&backend.operations[&id];
        (op.target.clone().unwrap(),backend.connection(&op.connection).unwrap().root.clone())};
    for info in &infos {
        let name=info.name();let Some(name)=name.to_str() else {continue};
        let mut backend=shared.borrow_mut();let op=backend.operations.get_mut(&id).unwrap();op.scanned+=1;
        if op.scanned>1000 {break;}
        if name.is_empty() || matches!(name,"."|"..") || name.contains(['/', '\0']) {continue;}
        let child=target.child(name);
        let Ok(uri)=address(&child.uri(),true) else {continue};
        let path=child.path().map(|path|path.to_string_lossy().into_owned()).unwrap_or_default();
        let path=if path.len()>4096 {String::new()}else{path};
        let display=info.display_name().chars().take(256).collect::<String>();
        op.rows.push(json!({"name":if display.is_empty(){name}else{&display},"uri":uri,"path":path,
            "directory":info.file_type()==gio::FileType::Directory,"size":info.size(),"modified":info.attribute_uint64("time::modified")}));
        if op.rows.len()>100 {break;}
    }
    let continuing={let backend=shared.borrow();let op=&backend.operations[&id];!infos.is_empty() && op.rows.len()<=100 && op.scanned<=1000};
    if continuing {next_batch(shared,id);return;}
    let (mut rows,scanned)={let mut backend=shared.borrow_mut();let op=backend.operations.get_mut(&id).unwrap();(std::mem::take(&mut op.rows),op.scanned)};
    rows.sort_by_key(|row|(!row["directory"].as_bool().unwrap_or(false),row["name"].as_str().unwrap_or("").to_lowercase()));
    let limited=rows.len()>100 || scanned>1000;rows.truncate(100);
    let parent=target.parent().filter(|parent|parent.equal(&root)||parent.has_prefix(&root))
        .and_then(|parent|address(&parent.uri(),true).ok()).unwrap_or_default();
    let uri=address(&target.uri(),true).unwrap_or_default();
    let path=target.path().map(|path|path.to_string_lossy().into_owned()).unwrap_or_default();
    finish(shared,id,Ok(json!({"rows":rows,"limited":limited,"uri":uri,"parent":parent,"path":path})));
}

fn valid_id(value:&str)->bool {value.len()==32 && value.bytes().all(|byte|byte.is_ascii_digit()||(b'a'..=b'f').contains(&byte))}
fn dispatch(shared:&Shared,request:&Value)->Result<()> {
    let data=request.as_object().ok_or("A network request is required")?;
    let id=request["id"].as_i64().filter(|value|(1..=9_007_199_254_740_991).contains(value)).ok_or("A positive request ID is required")?;
    let action=request["action"].as_str().ok_or("Unsupported network request fields")?;
    let allowed:&[&str]=match action {
        "status"=>&[],"connect"=>&["uri"],"reconnect"|"disconnect"=>&["connectionId"],
        "browse"=>&["connectionId","uri"],"cancel"=>&["target"],_=>return Err("Unsupported network request fields"),
    };
    if data.keys().any(|key|key!="id" && key!="action" && !allowed.contains(&key.as_str())) {
        return Err("Unsupported network request fields");
    }
    if action=="status" {refresh_mounts(shared,false);response(id,action,shared.borrow().snapshot());return Ok(());}
    if action=="cancel" {
        let target=request["target"].as_i64().ok_or("Choose an active request to cancel")?;
        cancel(shared,target,"The operation was cancelled");response(id,action,json!({"cancelled":target}));return Ok(());
    }
    if shared.borrow().operations.contains_key(&id){return Err("This network request ID is already active");}
    if shared.borrow().operations.len()>=8{return Err("Wait for a pending network operation or cancel it");}
    if matches!(action,"connect"|"reconnect") {
        let connection=if action=="connect" {
            let uri=address(request["uri"].as_str().ok_or("Enter a server address of at most 4096 bytes")?,false)?;
            let scheme=uri.split_once(':').unwrap().0;
            if !shared.borrow().protocols().contains(&scheme){return Err("This network protocol is unavailable; enable its GVfs provider in the host configuration");}
            let key=new_id().ok_or("The network provider could not complete the operation")?;
            let root=shared.borrow().vfs.file_for_uri(&uri);
            shared.borrow_mut().connections.push(Connection{id:key.clone(),uri:uri.clone(),name:host(&uri),state:"disconnected",
                path:String::new(),error:String::new(),root,mount:None,can_disconnect:false,generation:0});key
        } else {
            let key=request["connectionId"].as_str().filter(|value|valid_id(value)).ok_or("Choose a known network location")?.to_owned();
            {let backend=shared.borrow();let entry=backend.connection(&key).ok_or("This network location is no longer known")?;
                if matches!(entry.state,"connecting"|"authenticating"|"disconnecting"){return Err("This network location is busy");}
                let scheme=entry.uri.split_once(':').unwrap().0;
                if !backend.protocols().contains(&scheme){return Err("This network protocol is unavailable; enable its GVfs provider in the host configuration");}}
            key
        };
        shared.borrow_mut().connection_mut(&connection).unwrap().generation+=1;
        let cancellable=begin(shared,id,if action=="connect"{"connect"}else{"reconnect"},&connection,120,true)?;
        {let mut backend=shared.borrow_mut();let entry=backend.connection_mut(&connection).unwrap();
            entry.state="connecting";entry.error.clear();entry.path.clear();entry.can_disconnect=false;}
        changed(shared);
        let (root,toolkit)={let backend=shared.borrow();(backend.connection(&connection).unwrap().root.clone(),backend.operations[&id].toolkit.clone().unwrap())};
        let weak=Rc::downgrade(shared);
        root.mount_enclosing_volume(gio::MountMountFlags::NONE,Some(&toolkit),Some(&cancellable),move|result|{
            if let Some(shared)=weak.upgrade(){mounted(&shared,id,result);}
        });
        return Ok(());
    }
    let connection=request["connectionId"].as_str().filter(|value|valid_id(value)).ok_or("Choose a known network location")?;
    if shared.borrow().connection(connection).is_none(){return Err("This network location is no longer known");}
    match action {
        "disconnect"=>disconnect(shared,id,connection),
        "browse"=>{
            if !request["uri"].is_null() && !request["uri"].is_string(){return Err("Enter a valid server address");}
            browse(shared,id,connection,request["uri"].as_str())
        }
        _=>Err("Unsupported network action"),
    }
}

pub fn run() {
    unsafe {libc::umask(0o077);}
    let anonymous_fixture=std::env::args().skip(1).any(|arg|arg=="--anonymous-fixture");
    let shared=Rc::new(RefCell::new(Backend::new(anonymous_fixture)));
    let monitor=shared.borrow().monitor.clone();
    let weak=Rc::downgrade(&shared);
    monitor.connect_mount_added(move|_,_|{if let Some(shared)=weak.upgrade(){refresh_mounts(&shared,true);}});
    let weak=Rc::downgrade(&shared);
    monitor.connect_mount_removed(move|_,_|{if let Some(shared)=weak.upgrade(){refresh_mounts(&shared,true);}});
    let weak=Rc::downgrade(&shared);
    monitor.connect_mount_changed(move|_,_|{if let Some(shared)=weak.upgrade(){refresh_mounts(&shared,true);}});
    refresh_mounts(&shared,false);
    let mut ready=shared.borrow().snapshot();ready["event"]=json!("ready");emit(ready);
    let loop_=glib::MainLoop::new(None,false);
    let input_loop=loop_.clone();let input_state=shared.clone();let mut buffer=Vec::new();
    unsafe {let flags=libc::fcntl(0,libc::F_GETFL);if flags>=0{libc::fcntl(0,libc::F_SETFL,flags|libc::O_NONBLOCK);}}
    glib::unix_fd_add_local(0,glib::IOCondition::IN|glib::IOCondition::HUP,move|fd,_|{
        let mut chunk=[0_u8;65536];let size=unsafe{libc::read(fd,chunk.as_mut_ptr().cast(),chunk.len())};
        if size==0 {
            input_state.borrow_mut().closed=true;input_loop.quit();return glib::ControlFlow::Break;
        }
        if size<0{return glib::ControlFlow::Continue;}
        buffer.extend_from_slice(&chunk[..size as usize]);
        if buffer.len()>65536 && !buffer.contains(&b'\n') {
            input_state.borrow_mut().closed=true;input_loop.quit();return glib::ControlFlow::Break;
        }
        while let Some(end)=buffer.iter().position(|byte|*byte==b'\n') {
            let line=buffer.drain(..=end).collect::<Vec<_>>();
            if line.len()>65536 {emit(json!({"error":"Network request exceeds its size limit"}));continue;}
            let parsed=serde_json::from_slice::<Value>(&line);
            let id=parsed.as_ref().ok().and_then(|request|request["id"].as_i64());
            let result=parsed.as_ref().map_err(|_|"Invalid network request").and_then(|request|dispatch(&input_state,request));
            if let Err(error)=result {emit(json!({"id":id,"error":error}));}
        }
        glib::ControlFlow::Continue
    });
    loop_.run();
    let pending=shared.borrow().operations.keys().copied().collect::<Vec<_>>();
    for id in pending {cancel(&shared,id,"The operation was cancelled");}
}
