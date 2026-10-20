"""Optional isolated null-sink provider; never connects to host audio."""
import hashlib,json,os,subprocess,time
from pathlib import Path

def start_null_sink(package,env,output,processes):
    package=Path(package).resolve();output=Path(output)
    assert str(package).startswith('/nix/store/') and (package/'bin/pulseaudio').is_file()
    assert os.environ['HOME']=='/home/fixture' and not Path('/.old-root').exists() and not Path('/dev/snd').exists()
    socket=Path(env['XDG_RUNTIME_DIR'])/'private-pulse.sock';assert not socket.exists()
    command=[str(package/'bin/pulseaudio'),'--daemonize=no','--use-pid-file=no','--exit-idle-time=-1','--disable-shm=yes','--disallow-exit=yes','--realtime=no','-n','--load=module-null-sink sink_name=fixture','--load=module-native-protocol-unix socket='+str(socket)+' auth-anonymous=1']
    env['PULSE_SERVER']='unix:'+str(socket)
    with (output/'audio.log').open('w')as log:child=subprocess.Popen(command,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
    processes.append(child);deadline=time.monotonic()+5
    while not socket.exists():
        if child.poll()is not None or time.monotonic()>=deadline:raise RuntimeError('Private null-sink provider unavailable; see audio.log')
        time.sleep(.02)
    sinks=subprocess.check_output([str(package/'bin/pactl'),'--server='+env['PULSE_SERVER'],'list','short','sinks'],env=env,text=True,timeout=2)
    assert '\tfixture\t'in sinks and child.poll()is None,sinks
    (output/'audio-provider.json').write_text(json.dumps(dict(executable=str(package/'bin/pulseaudio'),sha256=hashlib.sha256((package/'bin/pulseaudio').read_bytes()).hexdigest(),argv=command,sinks=sinks,scope='Private null sink with no sound devices; no audio functionality acceptance'),indent=2))
    return child
