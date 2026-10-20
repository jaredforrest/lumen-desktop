"""Private same-process X11 window and DBusMenu exporter for native input tests."""
import argparse,json,os
from pathlib import Path
import dbus,dbus.service
from dbus.mainloop.glib import DBusGMainLoop
import gi
gi.require_version('Gtk','3.0');gi.require_version('GdkX11','3.0')
from gi.repository import Gtk,GdkX11
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args()
DBusGMainLoop(set_as_default=True);bus=dbus.SessionBus();name=dbus.service.BusName('org.lumen.TestMenuExporter',bus=bus)
class Menu(dbus.service.Object):
 revision=1
 disabled=False
 def node(self,id,label='',children=None,**properties):
  props={'label':label,'enabled':dbus.Boolean(not(self.disabled and id==13)),'visible':dbus.Boolean(True),**properties}
  if children is not None:props['children-display']='submenu'
  return dbus.Struct((dbus.Int32(id),dbus.Dictionary(props,signature='sv'),dbus.Array(children or [],signature='v')),signature='ia{sv}av')
 def layout(self):
  shortcut=dbus.Array([dbus.Array(['Control','K'],signature='s')],signature='as')
  return self.node(0,children=[self.node(10,'_File',children=[self.node(11,'Unavailable',enabled=dbus.Boolean(False)),self.node(12,type='separator'),self.node(13,'Checked Command',**{'toggle-type':'checkmark','toggle-state':dbus.Int32(1),'shortcut':shortcut}),self.node(14,'Nested',children=[self.node(141,'Deep Command',shortcut=shortcut)])]),self.node(15,'Disabled Heading',children=[],enabled=dbus.Boolean(False)),self.node(20,'_Edit',children=[self.node(21,'Edit Command')])]+[self.node(30+i,'Additional Heading '+str(i+1),children=[self.node(300+i,'Additional Command '+str(i+1))]) for i in range(8)])
 @dbus.service.method('com.canonical.dbusmenu',in_signature='iias',out_signature='u(ia{sv}av)')
 def GetLayout(self,parent,depth,properties):return dbus.UInt32(self.revision),self.layout()
 @dbus.service.method('com.canonical.dbusmenu',in_signature='i',out_signature='b')
 def AboutToShow(self,id):return False
 @dbus.service.method('com.canonical.dbusmenu',in_signature='isvu')
 def Event(self,id,event,data,timestamp):
  with (a.output/'events.jsonl').open('a') as log:log.write(json.dumps({'id':int(id),'revision':self.revision,'event':str(event),'pid':os.getpid()})+'\n')
 @dbus.service.signal('com.canonical.dbusmenu',signature='ui')
 def LayoutUpdated(self,revision,parent):pass
 @dbus.service.method('org.lumen.TestMenuExporter',in_signature='s')
 def Change(self,kind):
  self.revision=(self.revision+1)%(2**32)
  if kind=='disable':self.disabled=True
  self.LayoutUpdated(dbus.UInt32(self.revision),dbus.Int32(0))
 @dbus.service.method('org.lumen.TestMenuExporter')
 def HideWindow(self):window.hide()
 @dbus.service.method('org.lumen.TestMenuExporter')
 def ShowWindow(self):window.show_all()
 @dbus.service.method('org.lumen.TestMenuExporter',out_signature='s')
 def Status(self):return json.dumps({'revision':self.revision,'pid':os.getpid(),'keys':keys,'xid':xid})
menu=Menu(bus,'/Menu');keys=[]
window=Gtk.Window(title='Menu Exporter Fixture');window.set_default_size(540,360)
window.add(Gtk.Label(label='Private exported menu input'))
window.connect('key-press-event',lambda _,event:keys.append(int(event.keyval)) or False)
window.connect('destroy',Gtk.main_quit);window.show_all();xid=window.get_window().get_xid()
registrar=dbus.Interface(bus.get_object('com.canonical.AppMenu.Registrar','/com/canonical/AppMenu/Registrar'),'com.canonical.AppMenu.Registrar')
registrar.RegisterWindow(dbus.UInt32(xid),dbus.ObjectPath('/Menu'))
(a.output/'exporter.json').write_text(json.dumps({'pid':os.getpid(),'xid':xid}));Gtk.main()
