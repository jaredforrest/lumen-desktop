"""Read captured application semantics from an explicitly private accessibility bus."""
import json,time
import dbus
STATES={'enabled':8,'focusable':11,'focused':12,'sensitive':24,'showing':25,'visible':30}

class Observer:
 def __init__(self,address):
  if not address.startswith('unix:path=/run/fixture/'):raise ValueError('Explicit private accessibility bus required')
  self.bus=dbus.bus.BusConnection(address)
 def interface(self,reference,name):return dbus.Interface(self.bus.get_object(*reference,introspect=False),name)
 def pid(self,owner):return int(self.interface(('org.freedesktop.DBus','/org/freedesktop/DBus'),'org.freedesktop.DBus').GetConnectionUnixProcessID(owner,timeout=.5))
 def describe(self,reference):
  accessible=self.interface(reference,'org.a11y.atspi.Accessible');properties=self.interface(reference,'org.freedesktop.DBus.Properties')
  values={key:properties.Get('org.a11y.atspi.Accessible',key,timeout=.5) for key in ('Name','Description')};words=[int(value) for value in accessible.GetState(timeout=.5)]
  return dict(reference=list(map(str,reference)),pid=self.pid(reference[0]),name=str(values.get('Name','')),description=str(values.get('Description','')),role=str(accessible.GetRoleName(timeout=.5)),states={name:bool(words[index//32]&(1<<(index%32))) if index//32<len(words) else False for name,index in STATES.items()})
 def diagnostics(self):
  rows=[]
  for owner in self.bus.list_names():
   if not str(owner).startswith(':'):continue
   try:rows.append(dict(owner=str(owner),pid=self.pid(owner)))
   except dbus.DBusException as error:rows.append(dict(owner=str(owner),unavailable=str(error)))
  return rows
 def snapshot(self):
  pending=[(('org.a11y.atspi.Registry','/org/a11y/atspi/accessible/root'),[])];seen=set();rows=[];deadline=time.monotonic()+5
  while pending:
   if len(rows)>=2048 or time.monotonic()>deadline:raise ValueError('Private accessibility observation exceeded its budget')
   reference,parents=pending.pop(0);reference=tuple(map(str,reference))
   if reference in seen or reference[1]=='/org/a11y/atspi/null':continue
   seen.add(reference)
   try:
    row=self.describe(reference);row['ancestors']=parents;rows.append(row)
    if len(parents)>=48:raise ValueError('Private accessibility ancestry exceeds budget')
    children=self.interface(reference,'org.a11y.atspi.Accessible').GetChildren(timeout=.5)
    pending.extend((child,parents+[dict(reference=row['reference'],name=row['name'],role=row['role'])]) for child in children)
   except dbus.DBusException as error:rows.append(dict(reference=list(reference),unavailable=str(error)))
   if len(json.dumps(rows).encode())>4*1024**2:raise ValueError('Private accessibility snapshot exceeds byte budget')
  return rows
 def focus(self,row):
  current=self.describe(row['reference'])
  if any(current[key]!=row[key] for key in ('pid','name','role')) or not current['states']['showing'] or not current['states']['sensitive']:raise ValueError('Captured application control changed')
  if not self.interface(row['reference'],'org.a11y.atspi.Component').GrabFocus(timeout=.5):raise ValueError('Application control does not support accessible focus')
 def bounds(self,row):
  current=self.describe(row['reference'])
  if any(current[key]!=row[key] for key in ('pid','name','role','description')) or not current['states']['showing'] or not current['states']['sensitive']:raise ValueError('Captured application control changed')
  bounds=tuple(int(value) for value in self.interface(row['reference'],'org.a11y.atspi.Component').GetExtents(dbus.UInt32(0),timeout=.5))
  if len(bounds)!=4 or bounds[2]<=0 or bounds[3]<=0:raise ValueError('Application control has no painted extent')
  return bounds
 def close(self):self.bus.close()

def start(prefix,environment,launch,wait):
 session=dbus.bus.BusConnection(environment['DBUS_SESSION_BUS_ADDRESS'])
 try:
  launch([prefix/'libexec/at-spi-bus-launcher','--launch-immediately','--a11y=1','--screen-reader=1'],'a11y-bus.log')
  wait(lambda:session.name_has_owner('org.a11y.Bus'),'private accessibility bus owner')
  address=str(session.get_object('org.a11y.Bus','/org/a11y/bus').GetAddress(dbus_interface='org.a11y.Bus'))
  if not address.startswith('unix:path=/run/fixture/'):raise ValueError('Accessibility bus escaped private runtime')
  environment.update(AT_SPI_BUS_ADDRESS=address,GTK_A11Y='atspi',GTK_MODULES=str(prefix/'lib/gtk-2.0/modules/libatk-bridge.so'),NO_AT_BRIDGE='0')
  launch([prefix/'libexec/at-spi2-registryd'],'a11y-registry.log')
  observer=Observer(address);wait(lambda:observer.bus.name_has_owner('org.a11y.atspi.Registry'),'private accessibility registry')
  return observer
 finally:session.close()
