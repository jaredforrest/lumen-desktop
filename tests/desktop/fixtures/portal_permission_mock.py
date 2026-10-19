"""Disposable implementation of the documented PermissionStore D-Bus protocol."""
import argparse
import json
from pathlib import Path
import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib
p=argparse.ArgumentParser();p.add_argument('--ready',type=Path,required=True);a=p.parse_args()
DBusGMainLoop(set_as_default=True)
bus=dbus.SessionBus();name=dbus.service.BusName('org.freedesktop.impl.portal.PermissionStore',bus)
IFACE='org.freedesktop.impl.portal.PermissionStore'


class Store(dbus.service.Object):
    def __init__(self):
        super().__init__(bus,'/org/freedesktop/impl/portal/PermissionStore')
        self.version=2;self.ignore=False;self.blocked=''
        self.tables={
            'devices':{'camera':{'org.example.First':['yes'],'org.example.Second':['no'],'':['yes']}},
            'screenshot':{'screenshot':{'org.example.First':['ask'],'org.example.Unknown':['future-format']}},
            'screencast':{'private-restore-resource-one':{'org.example.First':['yes']},'private-restore-resource-two':{'org.example.First':['yes']}},
            'remote-desktop':{'private-remote-resource':{'org.example.Second':['yes']}},
        }
        self.calls=[]

    def entry(self,table,resource):
        if table==self.blocked:raise dbus.exceptions.DBusException('Fixture policy denied',name='org.freedesktop.DBus.Error.AccessDenied')
        if table not in self.tables or resource not in self.tables[table]:raise dbus.exceptions.DBusException('No entry',name='org.freedesktop.portal.Error.NotFound')
        return self.tables[table][resource]

    def permissions(self,entry):
        return dbus.Dictionary({app:dbus.Array(values,signature='s') for app,values in entry.items()},signature='sas')

    @dbus.service.method('org.freedesktop.DBus.Properties',in_signature='ss',out_signature='v')
    def Get(self,interface,key):
        assert interface==IFACE and key=='version'
        return dbus.UInt32(self.version)

    @dbus.service.method(IFACE,in_signature='s',out_signature='as')
    def List(self,table):
        if table==self.blocked:raise dbus.exceptions.DBusException('Fixture policy denied',name='org.freedesktop.DBus.Error.AccessDenied')
        return dbus.Array(list(self.tables.get(table,{})),signature='s')

    @dbus.service.method(IFACE,in_signature='ss',out_signature='a{sas}v')
    def Lookup(self,table,resource):
        return self.permissions(self.entry(table,resource)),dbus.String('PRIVATE_PROVIDER_METADATA_DO_NOT_EXPORT',variant_level=1)

    @dbus.service.method(IFACE,in_signature='sbssas',out_signature='')
    def SetPermission(self,table,create,resource,app,values):
        assert create is False or not bool(create)
        entry=self.entry(table,resource);self.calls.append(['SetPermission',str(table),str(resource),str(app),list(values)])
        if not self.ignore:
            entry[str(app)]=[str(value) for value in values];self.Changed(table,resource,False,dbus.String('private'),self.permissions(entry))

    @dbus.service.method(IFACE,in_signature='sss',out_signature='')
    def DeletePermission(self,table,resource,app):
        if self.version<2:raise dbus.exceptions.DBusException('Not supported',name='org.freedesktop.DBus.Error.UnknownMethod')
        entry=self.entry(table,resource);self.calls.append(['DeletePermission',str(table),str(resource),str(app)])
        if not self.ignore:
            entry.pop(str(app),None);self.Changed(table,resource,False,dbus.String('private'),self.permissions(entry))

    @dbus.service.signal(IFACE,signature='ssbva{sas}')
    def Changed(self,table,resource,deleted,data,permissions):pass

    @dbus.service.method('org.example.PermissionFixture',in_signature='',out_signature='s')
    def Dump(self):return json.dumps(dict(tables=self.tables,calls=self.calls))

    @dbus.service.method('org.example.PermissionFixture',in_signature='ubs',out_signature='')
    def Configure(self,version,ignore,blocked):self.version=int(version);self.ignore=bool(ignore);self.blocked=str(blocked)

store=Store();a.ready.write_text('ready');GLib.MainLoop().run()
