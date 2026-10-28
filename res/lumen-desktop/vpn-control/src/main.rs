//! Bounded JSON bridge to explicitly installed VPN clients.
use serde_json::{Value,json};
use std::{env,fs,io::{Read,Write},os::unix::fs::PermissionsExt,path::PathBuf,
    process::{Command,Stdio},thread,time::{Duration,Instant}};
type Result<T>=std::result::Result<T,String>;

fn which(name:&str)->Option<PathBuf>{
    if name.contains('/') {let path=PathBuf::from(name);return path.is_file().then_some(path);}
    for directory in env::split_paths(&env::var_os("PATH")?) {
        let path=directory.join(name);
        if path.is_file() && fs::metadata(&path).ok()?.permissions().mode()&0o111!=0{return Some(path);}
    }
    None
}
fn command(argv:&[&str],input:Option<&str>,seconds:u64)->Result<String>{
    let binary=argv.first().ok_or("Missing command")?;
    let path=which(binary).ok_or_else(||format!("{binary} is not installed"))?;
    let mut child=Command::new(&path).args(&argv[1..]).stdin(if input.is_some(){Stdio::piped()}else{Stdio::null()})
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e|e.to_string())?;
    let mut stdout=child.stdout.take().ok_or("Missing stdout")?;
    let mut stderr=child.stderr.take().ok_or("Missing stderr")?;
    let out=thread::spawn(move||{let mut data=Vec::new();stdout.read_to_end(&mut data).map(|_|data)});
    let err=thread::spawn(move||{let mut data=Vec::new();stderr.read_to_end(&mut data).map(|_|data)});
    if let Some(input)=input {child.stdin.take().ok_or("Missing stdin")?.write_all(input.as_bytes()).map_err(|e|e.to_string())?;}
    let deadline=Instant::now()+Duration::from_secs(seconds);
    while Instant::now()<deadline {
        if let Some(status)=child.try_wait().map_err(|e|e.to_string())? {
            let output=String::from_utf8_lossy(&out.join().map_err(|_|"Output reader failed")?.map_err(|e|e.to_string())?).to_string();
            let diagnostic=String::from_utf8_lossy(&err.join().map_err(|_|"Error reader failed")?.map_err(|e|e.to_string())?).to_string();
            if status.success(){return Ok(output);}
            let line=if diagnostic.is_empty(){&output}else{&diagnostic};
            let error=line.lines().last().map(|s|s.chars().take(240).collect()).unwrap_or_else(||format!("{binary} failed"));
            return Err(error);
        }
        thread::sleep(Duration::from_millis(15));
    }
    let _=child.kill();let _=child.wait();let _=out.join();let _=err.join();
    Err(format!("{binary} did not respond"))
}

fn text<'a>(value:&'a Value,key:&str)->&'a str{value[key].as_str().unwrap_or("")}
fn mullvad_status()->Value{
    let installed=which("mullvad").is_some();
    let mut status=json!({"installed":installed,"state":"unavailable","account":false,"expires":"","location":"","relay":""});
    if !installed{return status;}
    match command(&["mullvad","status","--json"],None,8).and_then(|s|serde_json::from_str::<Value>(&s).map_err(|e|e.to_string())) {
        Ok(data)=>{
            status["state"]=json!(data["state"].as_str().unwrap_or("unknown"));
            let location=&data["details"]["location"];
            status["location"]=json!([text(location,"city"),text(location,"country")]
                .into_iter().filter(|part|!part.is_empty()).collect::<Vec<_>>().join(", "));
        }
        Err(error)=>status["error"]=json!(error),
    }
    match command(&["mullvad","account","get"],None,8){
        Ok(output)=>{
            for line in output.lines(){
                let line=line.trim_start();
                let Some((key,value))=line.split_once(':') else{continue;};
                if ["mullvad account","account number"].contains(&key.trim().to_ascii_lowercase().as_str()){
                    status["account"]=json!(true);
                }
                if key.trim().eq_ignore_ascii_case("Expires at"){
                    let date=value.trim().get(..10).unwrap_or("");
                    if date.len()==10 && date.bytes().enumerate().all(|(i,c)|if i==4||i==7{c==b'-'}else{c.is_ascii_digit()}){
                        status["expires"]=json!(date);
                    }
                }
            }
            if status["account"]!=true{status["expires"]=json!("");}
        }
        Err(error)=>if !error.contains("Not logged in"){status["accountError"]=json!("Account status unavailable");},
    }
    if let Ok(output)=command(&["mullvad","relay","get"],None,8){
        if let Some(value)=output.lines().find_map(|line|line.trim_start().strip_prefix("Location:").map(str::trim)){
            status["relay"]=json!(value);
        }
    }
    status
}
fn tailscale_status()->Value{
    let installed=which("tailscale").is_some();
    let mut status=json!({"installed":installed,"state":"unavailable","online":false,"peers":0,
        "tailnet":"","exitNode":"","exitNodes":[],"health":[]});
    if !installed{return status;}
    match command(&["tailscale","status","--json"],None,8).and_then(|s|serde_json::from_str::<Value>(&s).map_err(|e|e.to_string())){
        Ok(data)=>{
            status["state"]=json!(data["BackendState"].as_str().unwrap_or("unknown"));
            status["online"]=json!(data["Self"]["Online"]==true && status["state"]=="Running");
            status["tailnet"]=json!(text(&data["CurrentTailnet"],"Name"));
            status["exitNode"]=json!(text(&data["ExitNodeStatus"],"ID"));
            let mut nodes=Vec::new();let mut peers=0;
            if let Some(items)=data["Peer"].as_object(){for peer in items.values(){
                if peer["Online"]==true{peers+=1;}
                let id=text(peer,"ID");
                if peer["ExitNodeOption"]==true && !id.is_empty(){
                    let name=[text(peer,"DNSName"),text(peer,"HostName"),"Exit node"].into_iter()
                        .find(|s|!s.is_empty()).unwrap_or("Exit node").trim_end_matches('.');
                    nodes.push(json!({"id":id,"name":name}));
                }
            }}
            nodes.sort_by_key(|node|text(node,"name").to_lowercase());
            status["peers"]=json!(peers);status["exitNodes"]=json!(nodes);
            status["health"]=json!(data["Health"].as_array().map(|rows|rows.iter().take(3).map(|row|
                row.as_str().unwrap_or("").chars().take(180).collect::<String>()).collect::<Vec<_>>()).unwrap_or_default());
        }
        Err(error)=>status["error"]=json!(error),
    }
    status
}
fn status()->Value{json!({"mullvad":mullvad_status(),"tailscale":tailscale_status()})}
fn tailscale_action(args:&[&str])->Result<()> {
    let mut command_args=vec!["tailscale"];command_args.extend_from_slice(args);
    match command(&command_args,None,25){Ok(_)=>Ok(()),Err(error)=>{
        let lower=error.to_ascii_lowercase();
        if !["access denied","permission denied","not authorized","requires root"].iter().any(|phrase|lower.contains(phrase)){
            return Err(error);
        }
        let binary=which("tailscale").ok_or(error.clone())?;
        if which("pkexec").is_none(){return Err(error);}
        let path=binary.to_str().ok_or("Invalid Tailscale path")?;
        let mut elevated=vec!["pkexec",path];elevated.extend_from_slice(args);
        command(&elevated,None,90).map(|_|())
    }}
}
fn perform(request:&Value)->Result<Value>{
    let action=request["action"].as_str().unwrap_or("status");
    match action{
        "status"=>return Ok(status()),
        "mullvad-connect"=>{command(&["mullvad","connect"],None,30)?;},
        "mullvad-disconnect"=>{command(&["mullvad","disconnect"],None,30)?;},
        "mullvad-login"=>{
            let account=text(request,"account");
            if account.len()!=16 || !account.bytes().all(|c|c.is_ascii_digit()){return Err("Enter a 16-digit Mullvad account number".into());}
            command(&["mullvad","account","login"],Some(&format!("{account}\n")),30)
                .map_err(|error|error.replace(account,"[account number]"))?;
        }
        "mullvad-location"=>{
            let country=text(request,"country");
            if country!="any" && (country.len()!=2 || !country.bytes().all(|c|c.is_ascii_lowercase())){
                return Err("Choose a valid country".into());
            }
            command(&["mullvad","relay","set","location",country],None,8)?;
        }
        "tailscale-connect"=>tailscale_action(&["up"] )?,
        "tailscale-disconnect"=>tailscale_action(&["down"] )?,
        "tailscale-exit"=>{
            let id=request["id"].as_str().ok_or("Invalid exit node")?;
            if !id.is_empty() && !tailscale_status()["exitNodes"].as_array().is_some_and(|nodes|nodes.iter().any(|node|node["id"]==id)){
                return Err("That exit node is no longer available".into());
            }
            let flag=format!("--exit-node={id}");
            tailscale_action(&["set",&flag])?;
        }
        _=>return Err("Unknown VPN action".into()),
    }
    Ok(status())
}
fn main(){
    let mut input=String::new();
    let result=std::io::stdin().read_line(&mut input).map_err(|e|e.to_string())
        .and_then(|_|serde_json::from_str::<Value>(&input).map_err(|e|e.to_string()))
        .and_then(|request|if request.is_object(){perform(&request)}else{Err("Invalid request".into())});
    println!("{}",match result{Ok(value)=>value,Err(error)=>json!({"error":error})});
}
#[cfg(test)] mod tests{
    use super::*;
    #[test]fn rejects_invalid_actions_before_side_effects(){
        for request in [json!({"action":"mullvad-login","account":"123"}),json!({"action":"mullvad-location","country":"gb;rm"}),
            json!({"action":"unknown"})]{assert!(perform(&request).is_err());}
    }
}
