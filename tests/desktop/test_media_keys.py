"""Media-key selection must tolerate empty sessions and never use the relay player."""
import importlib.util
from pathlib import Path
from unittest.mock import patch
source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime/media_key.py'
spec=importlib.util.spec_from_file_location('media_keys',source);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class Player:
 def __init__(self,name,playing=False,**properties):self.name=name;self.calls=[];self.properties=dict(CanControl=True,CanPause=True,CanPlay=True,CanGoNext=True,CanGoPrevious=True,PlaybackStatus='Playing' if playing else 'Paused',**properties)
 def GetAll(self,interface,**kwargs):return self.properties
 def Next(self,**kwargs):self.calls.append('next')
 def Previous(self,**kwargs):self.calls.append('previous')
 def PlayPause(self,**kwargs):self.calls.append('play-pause')
class Bus:
 def __init__(self,*players):self.players={p.name:p for p in players}
 def list_names(self):return list(self.players)
 def get_object(self,name,path):return self.players[name]
with patch.object(m.dbus,'Interface',side_effect=lambda obj,interface:obj):
 assert not m.dispatch(Bus(),'play-pause')
 relay=Player('org.mpris.MediaPlayer2.playerctld',True)
 assert not m.dispatch(Bus(relay),'next') and not relay.calls
 paused=Player('org.mpris.MediaPlayer2.a');playing=Player('org.mpris.MediaPlayer2.b',True)
 assert m.dispatch(Bus(paused,playing,relay),'next') and playing.calls==['next'] and not paused.calls
 playing.properties['CanGoPrevious']=False
 assert m.dispatch(Bus(paused,playing),'previous') and paused.calls==['previous']
 paused.properties['CanPlay']=False;playing.properties['CanPause']=False
 assert not m.dispatch(Bus(paused,playing),'play-pause')
 try:m.dispatch(Bus(),'delete')
 except ValueError:pass
 else:raise AssertionError('Invalid action accepted')
print('PASS media keys: empty session, relay exclusion, playing priority and capability enforcement')
