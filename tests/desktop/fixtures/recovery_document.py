"""Actual dirty GTK text document; private read-only status transport."""
import argparse,json,os
import dbus,dbus.service,dbus.mainloop.glib
import gi
gi.require_version('Gtk','3.0')
from gi.repository import Gtk,GLib
p=argparse.ArgumentParser();p.add_argument('--document',dest='name',choices=['A','B'],required=True);a=p.parse_args()
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
GLib.set_prgname('org.lumen.Recovery'+a.name)
window=Gtk.Window(title='Unsaved recovery document '+a.name);window.set_default_size(420,250)
view=Gtk.TextView();window.add(view);buffer=view.get_buffer()
class Document(dbus.service.Object):
    @dbus.service.method('org.lumen.RecoveryDocument',in_signature='',out_signature='s')
    def Status(self):return json.dumps(dict(pid=os.getpid(),name=a.name,modified=buffer.get_modified(),text=buffer.get_text(buffer.get_start_iter(),buffer.get_end_iter(),True),focused=view.has_focus()))
service=dbus.service.BusName('org.lumen.Recovery'+a.name,dbus.SessionBus());document=Document(service,'/Document')
window.connect('destroy',Gtk.main_quit);window.show_all();view.grab_focus();Gtk.main()
