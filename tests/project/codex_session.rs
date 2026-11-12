//! Exercise the production launcher against recording peers and a private socket.
use std::{env,fs,process::Command,os::unix::net::UnixListener,time::{SystemTime,UNIX_EPOCH}};
fn main(){
 let mut args=env::args().skip(1);let nu=args.next().unwrap();let source=args.next().unwrap();
 let root=env::temp_dir().join(format!("codex-session-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir(&root).unwrap();
 let result=std::panic::catch_unwind(||{
  let _socket=UnixListener::bind(root.join("wayland-test")).unwrap();
  let manager=root.join("manager.nu");let wrapped=root.join("wrapped.nu");let launcher=root.join("launch.nu");
  fs::write(&manager,format!("#!{nu}\ndef --wrapped main [...args] {{print 'WAYLAND_DISPLAY=wayland-test';print 'XDG_RUNTIME_DIR={}';print 'XDG_SESSION_TYPE=wayland'}}\n",root.display())).unwrap();
  fs::write(&wrapped,format!("#!{nu}\ndef --wrapped main [...args] {{print $\"DISPLAY=($env.WAYLAND_DISPLAY? | default '')\";for arg in $args {{print $\"ARG=($arg)\"}}}}\n")).unwrap();
  use std::os::unix::fs::PermissionsExt;
  for p in [&manager,&wrapped]{fs::set_permissions(p,fs::Permissions::from_mode(0o700)).unwrap();}
  let script=fs::read_to_string(source).unwrap().replace("@NU@",&nu).replace("@SYSTEMCTL@",manager.to_str().unwrap()).replace("@WRAPPED@",wrapped.to_str().unwrap());fs::write(&launcher,script).unwrap();
  let run=|args:&[&str],x11:bool|{
   let mut command=Command::new(&nu);command.arg(&launcher).args(args);
   for key in ["WAYLAND_DISPLAY","XDG_RUNTIME_DIR","XDG_SESSION_TYPE","DISPLAY","SWAYSOCK"]{command.env_remove(key);}
   if x11{command.env("XDG_SESSION_TYPE","x11");}
   let output=command.output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));String::from_utf8(output.stdout).unwrap()
  };
  let native=run(&["keep this space"],false);assert!(native.contains("DISPLAY=wayland-test\n")&&native.contains("ARG=--ozone-platform=wayland\n")&&native.contains("ARG=keep this space\n"),"{native}");
  let explicit=run(&["--ozone-platform=x11"],false);assert!(!explicit.contains("ARG=--ozone-platform=wayland")&&explicit.contains("ARG=--ozone-platform=x11"),"{explicit}");
  let cli=run(&["--cli"],false);assert!(!cli.contains("ARG=--ozone-platform")&&cli.contains("ARG=--cli"),"{cli}");
  let x11=run(&[],true);assert!(!x11.contains("ARG=--ozone-platform"),"{x11}");
 });
 fs::remove_dir_all(&root).unwrap();if let Err(error)=result{std::panic::resume_unwind(error);}
 println!("PASS Codex session recovery, native Wayland selection, explicit backend and CLI preservation");
}
