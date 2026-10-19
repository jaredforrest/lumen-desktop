"""Private synthetic MPRIS services with independent owners and recorded intents."""
import json,os,sys
import dbus,dbus.service,dbus.mainloop.glib
from gi.repository import GLib
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
ROOT='org.mpris.MediaPlayer2';PLAYER=ROOT+'.Player';PROPS='org.freedesktop.DBus.Properties'
players={};calls=[];generation=0
class Player(dbus.service.Object):
 def __init__(self,config):
  global generation
  generation+=1;self.generation=generation;self.name=ROOT+'.'+config['name'];self.bus_connection=dbus.bus.BusConnection(os.environ['DBUS_SESSION_BUS_ADDRESS']);self.owner=dbus.service.BusName(self.name,bus=self.bus_connection,do_not_queue=True)
  self.root=dict(CanQuit=False,CanRaise=False,HasTrackList=False,Identity=config.get('identity',config['name']),DesktopEntry='fixture',SupportedUriSchemes=dbus.Array([],signature='s'),SupportedMimeTypes=dbus.Array([],signature='s'))
  self.props=dict(PlaybackStatus='Paused',Rate=1.,Position=dbus.Int64(0),MinimumRate=1.,MaximumRate=1.,CanGoNext=True,CanGoPrevious=True,CanPlay=True,CanPause=True,CanSeek=True,CanControl=True,Volume=.5,Metadata=dbus.Dictionary({'mpris:trackid':dbus.ObjectPath('/fixture/track1'),'mpris:length':dbus.Int64(180000000),'xesam:title':'Synthetic title','xesam:artist':dbus.Array(['Synthetic artist'],signature='s')},signature='sv'))
  super().__init__(self.bus_connection,'/org/mpris/MediaPlayer2');self.configure(config.get('properties',{}),False)
 def configure(self,values,emit=True):
  for key,value in values.items():
   if value is None:self.props.pop(key,None)
   elif key=='Metadata':
    self.props[key]=dbus.Dictionary({k:dbus.ObjectPath(v) if k=='mpris:trackid' else dbus.Int64(v) if k=='mpris:length' else dbus.Array(v,signature='s') if isinstance(v,list) else v for k,v in value.items()},signature='sv')
   elif key=='Position':self.props[key]=dbus.Int64(value)
   else:self.props[key]=value
  if emit:self.PropertiesChanged(PLAYER,dbus.Dictionary({k:self.props[k] for k in values if k in self.props},signature='sv'),[k for k in values if k not in self.props])
 def record(self,method,*args):
  row=dict(name=self.name,generation=self.generation,method=method,args=list(args));calls.append(row)
  with open(os.environ['LUMEN_MPRIS_CALLS'],'a') as log:log.write(json.dumps(row)+'\n')
 @dbus.service.method(PROPS,in_signature='s',out_signature='a{sv}')
 def GetAll(self,interface):return dbus.Dictionary(self.root if interface==ROOT else self.props if interface==PLAYER else {},signature='sv')
 @dbus.service.method(PROPS,in_signature='ss',out_signature='v')
 def Get(self,interface,key):
  values=self.root if interface==ROOT else self.props
  if key not in values:raise dbus.exceptions.DBusException('Unavailable',name='org.freedesktop.DBus.Error.UnknownProperty')
  return values[key]
 @dbus.service.method(PROPS,in_signature='ssv')
 def Set(self,interface,key,value):self.record('Set',key,float(value));self.configure({key:value})
 @dbus.service.signal(PROPS,signature='sa{sv}as')
 def PropertiesChanged(self,interface,changed,invalidated):pass
 @dbus.service.signal(PLAYER,signature='x')
 def Seeked(self,position):pass
 @dbus.service.method(PLAYER)
 def Play(self):self.record('Play');self.configure({'PlaybackStatus':'Playing'})
 @dbus.service.method(PLAYER)
 def Pause(self):self.record('Pause');self.configure({'PlaybackStatus':'Paused'})
 @dbus.service.method(PLAYER)
 def PlayPause(self):self.record('PlayPause');self.configure({'PlaybackStatus':'Paused' if self.props['PlaybackStatus']=='Playing' else 'Playing'})
 @dbus.service.method(PLAYER)
 def Next(self):self.record('Next')
 @dbus.service.method(PLAYER)
 def Previous(self):self.record('Previous')
 @dbus.service.method(PLAYER,in_signature='ox')
 def SetPosition(self,track,position):self.record('SetPosition',str(track),int(position));self.props['Position']=position;self.Seeked(position)
 @dbus.service.method(PLAYER,in_signature='x')
 def Seek(self,offset):self.record('Seek',int(offset))
bus=dbus.SessionBus();name=dbus.service.BusName('org.lumen.MediaFixture',bus)
class Fixture(dbus.service.Object):
 @dbus.service.method('org.lumen.MediaFixture',in_signature='s')
 def Add(self,value):
  config=json.loads(value);assert config['name'] not in players;players[config['name']]=Player(config)
 @dbus.service.method('org.lumen.MediaFixture',in_signature='s')
 def Remove(self,key):
  player=players.pop(key);player.remove_from_connection();player.bus_connection.close()
 @dbus.service.method('org.lumen.MediaFixture',in_signature='ss')
 def Configure(self,key,value):players[key].configure(json.loads(value))
 @dbus.service.method('org.lumen.MediaFixture',out_signature='s')
 def Calls(self):return json.dumps(calls)
fixture=Fixture(bus,'/org/lumen/MediaFixture');GLib.MainLoop().run()
