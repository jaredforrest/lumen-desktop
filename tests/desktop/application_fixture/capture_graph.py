"""Synthetic audio graph only; callers must establish the application isolation boundary."""
import json,os,subprocess,time
from pathlib import Path

CONFIG='''context.properties = { core.daemon = true core.name = capture-fixture support.dbus = false }
context.spa-libs = { audio.convert.* = audioconvert/libspa-audioconvert audio.adapt = audioconvert/libspa-audioconvert support.* = support/libspa-support audiotestsrc = audiotestsrc/libspa-audiotestsrc }
context.modules = [ { name = libpipewire-module-protocol-native } { name = libpipewire-module-metadata } { name = libpipewire-module-spa-node-factory } { name = libpipewire-module-client-node } { name = libpipewire-module-access } { name = libpipewire-module-adapter } { name = libpipewire-module-link-factory } ]
context.objects = [ { factory = spa-node-factory args = { factory.name = support.node.driver node.name = Synthetic-Driver node.group = pipewire.dummy priority.driver = 20000 } } ]
'''

class SyntheticCaptureGraph:
 def __init__(self,binary,runtime,output,environment):
  self.binary,self.runtime,self.output=map(Path,(binary,runtime,output));self.env=dict(environment,PIPEWIRE_REMOTE='capture-fixture',PIPEWIRE_RUNTIME_DIR=str(runtime));self.child=None
 def run(self,*arguments):return subprocess.check_output([str(self.binary/arguments[0]),*arguments[1:]],env=self.env,text=True,timeout=4).strip()
 def graph(self):return json.loads(self.run('pw-dump','--no-colors'))
 def start(self):
  config=self.output/'pipewire.conf';config.write_text(CONFIG)
  with (self.output/'pipewire.log').open('w') as stream:self.child=subprocess.Popen([str(self.binary/'pipewire'),'-c',str(config)],env=self.env,stdout=stream,stderr=subprocess.STDOUT)
  end=time.monotonic()+8
  while not (self.runtime/'capture-fixture').is_socket():
   if self.child.poll() is not None:raise AssertionError('Private PipeWire exited')
   if time.monotonic()>end:raise AssertionError('Private PipeWire socket did not appear')
   time.sleep(.02)
 def prepare(self):
  self.run('pw-cli','create-node','adapter','{ factory.name = audiotestsrc node.name = synthetic-source media.class = Audio/Source audio.position = [ MONO ] adapter.auto-port-config = { mode = dsp monitor = false position = preserve } object.linger = true }')
  self.run('pw-cli','create-node','adapter','{ factory.name = support.null-audio-sink node.name = synthetic-capture media.class = Stream/Input/Audio audio.position = [ MONO ] adapter.auto-port-config = { mode = dsp monitor = false position = preserve } object.linger = true }')
  nodes=self.graph();source=next(node['id'] for node in nodes if node.get('info',{}).get('props',{}).get('node.name')=='synthetic-source')
  self.run('pw-cli','set-param',str(source),'PortConfig','{ direction = Output mode = dsp format = { mediaType = audio mediaSubtype = raw format = F32 rate = 48000 channels = 1 position = [ MONO ] } }')
  self.source=next(value for value in self.run('pw-link','-o').splitlines() if value.startswith('synthetic-source:'));self.target=next(value for value in self.run('pw-link','-i').splitlines() if value.startswith('synthetic-capture:'))
 def link(self):self.run('pw-link',self.source,self.target)
 def unlink(self):self.run('pw-link','-d',self.source,self.target)
 def close(self):
  if self.child is not None:
   if self.child.poll() is None:self.child.terminate()
   try:self.child.wait(timeout=3)
   except subprocess.TimeoutExpired:self.child.kill();self.child.wait(timeout=3)
   self.child=None
