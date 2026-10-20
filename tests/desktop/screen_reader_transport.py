"""Actual Speech Dispatcher acceptance, completion and cancel on a private socket."""
import argparse,json,os,shutil,subprocess as sp,sys,tempfile,time
from pathlib import Path
from screen_reader_speech import configure,source_proof
p=argparse.ArgumentParser();p.add_argument('--dispatcher',type=Path,required=True);p.add_argument('--dispatcher-source',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False);(a.output/'source-proof.json').write_text(json.dumps(source_proof(a.dispatcher_source),indent=2))
with tempfile.TemporaryDirectory(prefix='lumen-speech-transport-') as directory:
 root=Path(directory);env=dict(os.environ,HOME=directory,XDG_RUNTIME_DIR=directory,DBUS_SESSION_BUS_ADDRESS='unix:path='+directory+'/no-session',DBUS_SYSTEM_BUS_ADDRESS='unix:path='+directory+'/no-system',PULSE_SERVER='unix:'+directory+'/no-audio',ALSA_CONFIG_PATH=str(root/'no-alsa'))
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 settings=configure(a.output,a.dispatcher,shutil.which('true'));env['SPEECHD_ADDRESS']=settings['address'];env['PATH']=str(settings['emptyBin'])
 with (a.output/'dispatcher.log').open('w') as log:process=sp.Popen(settings['command'],env=env,stdout=log,stderr=sp.STDOUT)
 try:
  deadline=time.monotonic()+5
  while not settings['socket'].exists() and time.monotonic()<deadline:
   assert process.poll() is None;time.sleep(.02)
  assert settings['socket'].exists()
  sys.path.insert(0,str(next((a.dispatcher/'lib').glob('python*/site-packages'))));import speechd
  events=[];cancel_events=[];client=speechd.SSIPClient('PrivateObserver',address=settings['address'])
  try:
   marker=root/'must-not-exist';payload="Transport sentinel ' $(touch "+str(marker)+") `touch "+str(marker)+"`\nsecond line"
   client.set_output_module('observer');client.speak(payload,callback=lambda *args,**kwargs:events.append((str(args),str(kwargs))),event_types=(speechd.CallbackType.BEGIN,speechd.CallbackType.END,speechd.CallbackType.CANCEL))
   deadline=time.monotonic()+5
   while not any('end' in str(event).lower() for event in events) and time.monotonic()<deadline:time.sleep(.02)
   assert any('end' in str(event).lower() for event in events),events
   client.speak('Cancel sentinel '*1024,callback=lambda *args,**kwargs:cancel_events.append((str(args),str(kwargs))),event_types=(speechd.CallbackType.BEGIN,speechd.CallbackType.END,speechd.CallbackType.CANCEL))
   deadline=time.monotonic()+5
   while not any('begin' in str(event).lower() for event in cancel_events) and time.monotonic()<deadline:time.sleep(.005)
   client.cancel()
   deadline=time.monotonic()+5
   while not any('cancel' in str(event).lower() for event in cancel_events) and time.monotonic()<deadline:time.sleep(.005)
   assert any('cancel' in str(event).lower() for event in cancel_events),cancel_events
   assert not marker.exists(),'Speech payload escaped fixed ignored argument'
  finally:client.close()
  protocol=settings['protocol'].read_text();assert 'Transport sentinel' in protocol,protocol
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,events=events,cancelEvents=cancel_events,checks=['real Speech Dispatcher Unix transport','fixed generic no-audio module','actual text accepted','completion callbacks','private cancellation']),indent=2));print('PASS real private speech transport',events)
 finally:
  if process.poll() is None:
   process.terminate()
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:process.kill();process.wait(timeout=5)
