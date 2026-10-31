use lumen_application_lifecycle::{Lifecycle,ObservationStore,Result,epoch,receive,send,window_nodes};
use lumen_application_lifecycle::cooperative::Cooperative;
use lumen_application_lifecycle::session::SessionEnd;
use serde_json::{Value,json};
use std::fs;
use std::io::{Read,Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration,Instant};

fn emit(available:bool,lifecycle:&Lifecycle,session:&SessionEnd,store:&mut Option<ObservationStore>,last:&mut Option<String>,error:&str,result:Option<Value>){
    let mut error=error.to_owned();let mut recovery=false;
    if available{
        if let Some(store)=store{
            match store.save_complete(&lifecycle.records(),&lifecycle.activation_history,&lifecycle.window_history){
                Ok(())=>recovery=true,
                Err(_)=>{let _=store.invalidate();error="Application observations could not be saved for shell recovery.".into();}
            }
        }
    }
    let observation=if available&&lifecycle.native_guard_available{lifecycle.native_observation()}else{None};
    let mut value=json!({"nativeObservation":observation,"available":available,"applications":if available{lifecycle.rows()}else{Vec::new()},"activationHistory":if available{lifecycle.activation_history.order()}else{Vec::new()},"nativeGuardAvailable":available&&lifecycle.native_guard_available,"nativeWindows":if available&&lifecycle.native_guard_available{lifecycle.window_history.captures(lifecycle)}else{Vec::new()},"nativeCoverageComplete":available&&lifecycle.native_guard_available&&lifecycle.native_coverage_complete,"nativeFocused":if available{lifecycle.current_source_focus.clone()}else{Value::Null},"sessionEpoch":if available{lifecycle.epoch.clone()}else{None},"error":error,"recoveryAvailable":recovery,"session":session.state()});
    if let Some(result)=result{value["result"]=result;}
    if let Ok(encoded)=serde_json::to_string(&value){
        if last.as_ref()!=Some(&encoded){println!("{encoded}");let _=std::io::stdout().flush();*last=Some(encoded);}
    }
}
fn refresh(query:&mut UnixStream,path:&PathBuf,expected:&str,lifecycle:&mut Lifecycle,session:&mut SessionEnd,protocol:&Cooperative,store:&mut Option<ObservationStore>,last:&mut Option<String>)->Result<Vec<Value>>{
    let nodes=sample_until(query,path,expected,lifecycle,session,protocol,Instant::now()+Duration::from_secs(5))?;
    emit(true,lifecycle,session,store,last,"",None);Ok(nodes)
}
fn sample_until(query:&mut UnixStream,path:&PathBuf,expected:&str,lifecycle:&mut Lifecycle,session:&mut SessionEnd,protocol:&Cooperative,deadline:Instant)->Result<Vec<Value>>{
    let tree=lumen_application_lifecycle::events::tree(query,deadline)?;
    if epoch(path)?!=expected{return Err("Compositor session changed".into());}
    let nodes=window_nodes(&tree)?;
    lifecycle.refresh(expected,&nodes);session.tick(&nodes,lifecycle,protocol);
    Ok(nodes)
}
// None means compositor shutdown; errors retire the observer before dispatch.
fn pending(events:&mut UnixStream,nodes:&mut Vec<Value>,query:&mut UnixStream,path:&PathBuf,expected:&str,lifecycle:&mut Lifecycle,session:&mut SessionEnd,protocol:&Cooperative,store:&mut Option<ObservationStore>)->Result<Option<bool>>{
    let deadline=Instant::now()+Duration::from_millis(250);
    let batch=lumen_application_lifecycle::events::drain(events)?;
    let any=!batch.is_empty();
    for (kind,event) in batch {
        if kind==0x8000_0006{if let Some(store)=store{store.save(&[])?;}return Ok(None);}
        let window_event=lumen_application_lifecycle::events::window_event(kind)?;
        if window_event{lifecycle.inventory.observe_event(&event);lifecycle.observe(&event["container"]);}
        else {lifecycle.inventory.observe_scope_event();}
        let remaining=deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero(){return Err("Event processing budget exhausted".into());}
        // Keep every invalidation, sample and focus receipt ordered, but do not
        // repeat durable journal fsync/publication inside the sampling budget.
        // The enclosing observation/action emits once after the whole batch.
        let updated=sample_until(query,path,expected,lifecycle,session,protocol,deadline);
        *nodes=updated?;if window_event{lifecycle.observe_focus_event(&event);}
        if Instant::now()>=deadline{return Err("Event processing budget exhausted".into());}
    }
    Ok(Some(any))
}
fn handle(data:&Value,events:&mut UnixStream,nodes:&mut Vec<Value>,query:&mut UnixStream,path:&PathBuf,expected:&str,lifecycle:&mut Lifecycle,session:&mut SessionEnd,protocol:&Cooperative,store:&mut Option<ObservationStore>,last:&mut Option<String>)->Result<()>{
    let action=data["action"].as_str().unwrap_or("");
    if action=="status"{emit(true,lifecycle,session,store,last,"",None);return Ok(());}
    if action=="session_cancel"{session.cancel();emit(true,lifecycle,session,store,last,"",None);return Ok(());}
    if action=="focus_native"{
        let request=data["requestId"].as_str().filter(|id|!id.is_empty()&&id.len()<=128&&!id.chars().any(char::is_control));
        if !lifecycle.native_guard_available {
            emit(true,lifecycle,session,store,last,"This compositor does not support guarded window activation.",Some(json!({"nativeFocus":{"requestId":request.unwrap_or(""),"requested":false,"unsupported":true,"error":"This compositor does not support guarded window activation. Window-based controls remain available."}})));return Ok(());
        }
        let mut uncertain=false;
        let mut retire=false;
        let outcome=(||->Result<Value>{
            let request=request.ok_or("Invalid focus request")?;
            let switch_space=data["switchSpace"].as_bool().ok_or("Invalid workspace preference")?;
            let mut passes=0;
            let sampling_deadline=Instant::now()+Duration::from_secs(5);
            let tree=loop {
                passes+=1;
                if passes>3{retire=true;return Err("Compositor remained active during bounded focus barrier".into());}
                pending(events,nodes,query,path,expected,lifecycle,session,protocol,store).map_err(|e|{retire=true;e})?.ok_or_else(||{retire=true;"Compositor shut down".to_owned()})?;
                let tree=lumen_application_lifecycle::events::tree(query,sampling_deadline).map_err(|e|{retire=true;e})?;
                if epoch(path)?!=expected{return Err("Compositor session changed".into());}
                *nodes=window_nodes(&tree)?;lifecycle.refresh(expected,nodes);
                let changed=pending(events,nodes,query,path,expected,lifecycle,session,protocol,store).map_err(|e|{retire=true;e})?.ok_or_else(||{retire=true;"Compositor shut down".to_owned()})?;
                if !changed {break tree;}
            };
            let inventory=data.get("inventoryWitness").ok_or("Missing captured inventory witness")?;
            lifecycle.require_inventory(inventory)?;
            let initial=data.get("initialFocus").ok_or("Missing captured source focus")?;
            if !lifecycle.source_focus_matches(initial){return Err("Native focus changed after preview".into());}
            let plan=lifecycle.native_focus_plan(&data["capture"],&tree,switch_space)?;
            if epoch(path)?!=expected{return Err("Compositor session changed".into());}
            // Kernel-process witnesses are sampled. The typed compositor guard
            // independently checks actual source/surface topology at dispatch.
            lifecycle.native_focus_plan(&data["capture"],&tree,switch_space)?;
            lifecycle.require_inventory(inventory)?;
            let packet=lumen_window_policy::guard::packet(&tree,initial,&plan.target,&plan.actions)?;
            let command=lumen_window_policy::guard::command(&packet)?;
            uncertain=true;send(query,0,&command)?;let (kind,reply)=receive(query)?;
            if kind!=0||!reply.as_array().is_some_and(|rows|rows.len()==1&&rows.iter().all(|row|row["success"]==true)){
                if kind==0&&reply.as_array().is_some_and(|rows|rows.len()==1&&rows[0]["error"].as_str().is_some_and(|error|error.starts_with("LFG_REJECTED:"))){uncertain=false;}
                return Err("The compositor rejected guarded activation".into());
            }
            uncertain=false;
            Ok(json!({"requestId":request,"requested":plan.focus_allowed,"conId":plan.target["id"],"foreignId":plan.target["foreign_toplevel_identifier"]}))
        })();
        let result=outcome.unwrap_or_else(|_|json!({"requestId":request.unwrap_or(""),"requested":false,"uncertain":uncertain,"error":if uncertain{"The window action may have partially completed. No retry was attempted; check the desktop before opening the switcher again."}else{"The captured window, process or desktop state changed. Open the switcher again."}}));
        emit(true,lifecycle,session,store,last,"",Some(json!({"nativeFocus":result})));return if retire{Err("Bounded compositor observation retired".into())}else{Ok(())};
    }
    let result=(||->Result<Option<Value>>{
        if matches!(action,"session_begin"|"session_authorize"){
            *nodes=refresh(query,path,expected,lifecycle,session,protocol,store,last)?;
            if action=="session_begin"{
                session.begin(data["sessionAction"].as_str().unwrap_or(""),nodes,lifecycle,protocol)?;
                emit(true,lifecycle,session,store,last,"",None);return Ok(None);
            }
            let authorization=session.authorize(data["token"].as_str().unwrap_or(""),nodes,lifecycle,protocol)?;
            return Ok(Some(json!({"sessionAuthorization":authorization})));
        }
        if action!="quit"{return Err("Unsupported application command".into());}
        let app=data["appId"].as_str().ok_or("Unsupported application command")?;
        protocol.quit(lifecycle,app,data.get("instanceIds")).map(Some)
    })();
    match result{
        Ok(Some(result))=>emit(true,lifecycle,session,store,last,"",Some(result)),
        Ok(None)=>(),
        Err(_)=>{
            if action.starts_with("session_"){session.fail("Session confirmation failed. The session remains open; cancel and start again.");}
            let result=json!({"requested":false,"error":"Cooperative Quit is unavailable or the application changed. Use its own Quit command; Force Quit is a separate action."});
            emit(true,lifecycle,session,store,last,"",Some(result));
        }
    }
    Ok(())
}
fn watch()->Result<()>{
    let mut lifecycle=Lifecycle::default();lifecycle.bind_shell_owner(unsafe{libc::getppid()} as u32);let protocol=Cooperative;
    let mut session=SessionEnd::new(unsafe{libc::getppid()} as u32);
    let mut store:Option<ObservationStore>=None;let mut last=None;
    emit(false,&lifecycle,&session,&mut store,&mut last,"",None);
    let result=(||->Result<()>{
        let path=PathBuf::from(std::env::var_os("SWAYSOCK").ok_or("No supported compositor session")?);
        let identity=epoch(&path)?;
        let boot_id=fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(|error|error.to_string())?;
        let runtime=PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("No private desktop runtime directory")?);
        store=Some(ObservationStore::new(&runtime,&identity,boot_id.trim())?);
        let mut events=UnixStream::connect(&path).map_err(|error|error.to_string())?;
        events.set_read_timeout(Some(Duration::from_secs(5))).map_err(|error|error.to_string())?;
        send(&mut events,2,"[\"window\",\"workspace\",\"output\",\"shutdown\"]")?;
        let (kind,ack)=receive(&mut events)?;
        if kind!=2||ack["success"]!=true{return Err("Compositor subscription failed".into());}
        let mut query=UnixStream::connect(&path).map_err(|error|error.to_string())?;
        query.set_read_timeout(Some(Duration::from_secs(5))).map_err(|error|error.to_string())?;
        query.set_write_timeout(Some(Duration::from_secs(5))).map_err(|error|error.to_string())?;
        if let Some(store)=&mut store{let saved=store.load_state();lifecycle.recover(&identity,&saved.processes);lifecycle.activation_history=saved.history;lifecycle.window_history=saved.windows;lifecycle.prune();}
        send(&mut query,7,"")?;let (version_kind,version)=receive(&mut query)?;
        lifecycle.native_guard_available=version_kind==7&&version["native_focus_guard"].as_u64()==Some(2);
        let mut nodes=refresh(&mut query,&path,&identity,&mut lifecycle,&mut session,&protocol,&mut store,&mut last)?;
        lifecycle.observe_focus(&nodes);emit(true,&lifecycle,&session,&mut store,&mut last,"",None);
        events.set_read_timeout(None).map_err(|error|error.to_string())?;
        let mut input=Vec::new();
        loop{
            let mut polls=vec![libc::pollfd{fd:events.as_raw_fd(),events:libc::POLLIN,revents:0},libc::pollfd{fd:libc::STDIN_FILENO,events:libc::POLLIN,revents:0}];
            for process in lifecycle.processes.values(){polls.push(libc::pollfd{fd:process.handle.as_raw_fd(),events:libc::POLLIN,revents:0});}
            let status=unsafe{libc::poll(polls.as_mut_ptr(),polls.len() as libc::nfds_t,1000)};
            if status<0{if std::io::Error::last_os_error().kind()==std::io::ErrorKind::Interrupted{continue;}return Err("Application observation unavailable".into());}
            if polls[0].revents!=0{
                if pending(&mut events,&mut nodes,&mut query,&path,&identity,&mut lifecycle,&mut session,&protocol,&mut store)?.is_none(){eprintln!("Application observer stopped: compositor shutdown");return Ok(());}
            }
            if polls[1].revents!=0{
                let mut chunk=[0_u8;65537];let count=std::io::stdin().read(&mut chunk).map_err(|error|error.to_string())?;
                if count==0{eprintln!("Application observer stopped: request stream closed");return Ok(());}
                input.extend_from_slice(&chunk[..count]);
                if input.len()>65536&&!input.contains(&b'\n'){return Err("Application request exceeds limit".into());}
                while let Some(position)=input.iter().position(|byte|*byte==b'\n'){
                    let line=input.drain(..=position).collect::<Vec<_>>();
                    if line.len()>65537{return Err("Application request exceeds limit".into());}
                    let data:Value=serde_json::from_slice(&line).unwrap_or(Value::Null);
                    handle(&data,&mut events,&mut nodes,&mut query,&path,&identity,&mut lifecycle,&mut session,&protocol,&mut store,&mut last)?;
                }
            }
            lifecycle.prune();session.tick(&nodes,&mut lifecycle,&protocol);emit(true,&lifecycle,&session,&mut store,&mut last,"",None);
        }
    })();
    if result.is_err(){emit(false,&lifecycle,&session,&mut store,&mut last,"Application lifecycle observation is unavailable; window-based state remains available.",None);}
    session.cancel();lifecycle.close();result
}
fn main(){if let Err(error)=watch(){eprintln!("Application observer retired: {}",error.chars().take(256).collect::<String>());}}
#[cfg(test)] mod pending_tests;
