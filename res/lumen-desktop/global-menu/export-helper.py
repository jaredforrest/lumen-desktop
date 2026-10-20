"""DBusMenu transport for the opt-in Electron menu adapter, owned by its parent."""
import json,sys
import dbus,dbus.service,dbus.mainloop.glib
from gi.repository import GLib
MENU='com.canonical.dbusmenu'
def layout(n):
 p=dict(n.get('props',{}))
 if 'shortcut' in p:p['shortcut']=dbus.Array([dbus.Array(s,signature='s') for s in p['shortcut']],signature='as')
 return dbus.Struct((dbus.Int32(n['id']),dbus.Dictionary(p,signature='sv'),dbus.Array([layout(c) for c in n.get('children',[])],signature='v')),signature='ia{sv}av',variant_level=1)
class Export(dbus.service.Object):
 def __init__(self,bus,xid):
  self.xid=xid;self.tree={'id':0};self.rev=0;self.path='/adapter/window'+str(xid)
  super().__init__(bus,self.path)
 def register(self):
  try:
   dbus.Interface(bus.get_object('com.canonical.AppMenu.Registrar','/com/canonical/AppMenu/Registrar',introspect=False),'com.canonical.AppMenu.Registrar').RegisterWindow(dbus.UInt32(self.xid),self.path)
   print(json.dumps({'registered':True,'xid':self.xid}),flush=True)
  except dbus.DBusException:pass
 @dbus.service.method(MENU,in_signature='iias',out_signature='u(ia{sv}av)')
 def GetLayout(self,parent,depth,properties):return dbus.UInt32(self.rev),layout(self.tree)
 @dbus.service.method(MENU,in_signature='i',out_signature='b')
 def AboutToShow(self,ident):return False
 @dbus.service.signal(MENU,signature='ui')
 def LayoutUpdated(self,revision,parent):pass
 @dbus.service.method(MENU,in_signature='isvu',out_signature='')
 def Event(self,ident,event,data,timestamp):
  if event=='clicked':print(json.dumps({'xid':self.xid,'id':int(ident)}),flush=True)
def read(source,condition):
 if condition & GLib.IO_HUP:loop.quit();return False
 try:
  state=json.loads(source.readline());seen=set()
  for w in state:
   xid=w['xid'];seen.add(xid)
   if xid not in exports:exports[xid]=Export(bus,xid);exports[xid].register()
   e=exports[xid];e.tree=w['menu'];e.rev+=1;e.LayoutUpdated(e.rev,0)
  for xid in set(exports)-seen:
   e=exports.pop(xid)
   try:dbus.Interface(bus.get_object('com.canonical.AppMenu.Registrar','/com/canonical/AppMenu/Registrar',introspect=False),'com.canonical.AppMenu.Registrar').UnregisterWindow(dbus.UInt32(xid))
   except dbus.DBusException:pass
   e.remove_from_connection()
 except (ValueError,KeyError) as ex:print(str(ex),file=sys.stderr)
 return True
def owner(name,old,new):
 if name=='com.canonical.AppMenu.Registrar':
  if new:
   for e in exports.values():e.register()
  else:print(json.dumps({'registered':False}),flush=True)
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus=dbus.SessionBus();exports={};loop=GLib.MainLoop()
bus.add_signal_receiver(owner,signal_name='NameOwnerChanged',dbus_interface='org.freedesktop.DBus')
GLib.io_add_watch(sys.stdin, GLib.PRIORITY_DEFAULT, GLib.IO_IN|GLib.IO_HUP,read);loop.run()
