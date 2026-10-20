"""One-device BlueZ agent; PIN/confirmation requests require an explicit UI reply."""
import json
import re
import sys
import dbus
import dbus.service
import dbus.mainloop.glib
from gi.repository import GLib


def emit(data):
    print(json.dumps(data),flush=True)


class Agent(dbus.service.Object):
    def __init__(self,bus,device,loop):
        super().__init__(bus,'/org/lumendesktop/Pairing')
        self.device,self.loop,self.pending=device,loop,None

    def ask(self,device,kind,reply,error,**details):
        if str(device)!=self.device or self.pending:
            error(dbus.DBusException('Unexpected pairing request',name='org.bluez.Error.Rejected'));return
        self.pending=(kind,reply,error)
        emit(dict(event='prompt',kind=kind,**details))

    @dbus.service.method('org.bluez.Agent1',in_signature='o',out_signature='s',async_callbacks=('reply','error'))
    def RequestPinCode(self,device,reply,error):self.ask(device,'pin',reply,error)

    @dbus.service.method('org.bluez.Agent1',in_signature='o',out_signature='u',async_callbacks=('reply','error'))
    def RequestPasskey(self,device,reply,error):self.ask(device,'passkey',reply,error)

    @dbus.service.method('org.bluez.Agent1',in_signature='ou',out_signature='',async_callbacks=('reply','error'))
    def RequestConfirmation(self,device,passkey,reply,error):self.ask(device,'confirm',reply,error,code=f'{int(passkey):06d}')

    @dbus.service.method('org.bluez.Agent1',in_signature='o',out_signature='',async_callbacks=('reply','error'))
    def RequestAuthorization(self,device,reply,error):self.ask(device,'authorize',reply,error)

    @dbus.service.method('org.bluez.Agent1',in_signature='os',out_signature='',async_callbacks=('reply','error'))
    def AuthorizeService(self,device,uuid,reply,error):self.ask(device,'authorize',reply,error)

    @dbus.service.method('org.bluez.Agent1',in_signature='os',out_signature='')
    def DisplayPinCode(self,device,pin):
        if str(device)==self.device:emit(dict(event='prompt',kind='display',code=str(pin)))

    @dbus.service.method('org.bluez.Agent1',in_signature='ouq',out_signature='')
    def DisplayPasskey(self,device,passkey,entered):
        if str(device)==self.device:emit(dict(event='prompt',kind='display',code=f'{int(passkey):06d}',entered=int(entered)))

    @dbus.service.method('org.bluez.Agent1',in_signature='',out_signature='')
    def Cancel(self):
        self.pending=None;emit(dict(event='cancelled'))

    @dbus.service.method('org.bluez.Agent1',in_signature='',out_signature='')
    def Release(self):self.loop.quit()

    def respond(self,data):
        if not self.pending:return
        kind,reply,error=self.pending
        if not data.get('accept'):
            self.pending=None;error(dbus.DBusException('Pairing rejected',name='org.bluez.Error.Rejected'));return
        value=str(data.get('value',''))
        if kind=='pin' and not 1<=len(value)<=16:raise ValueError('Enter 1–16 characters')
        if kind=='passkey' and not re.fullmatch(r'[0-9]{1,6}',value):raise ValueError('Enter a passkey of up to six digits')
        self.pending=None
        if kind=='pin':reply(value)
        elif kind=='passkey':reply(dbus.UInt32(int(value)))
        else:reply()


def main(device):
    if not re.fullmatch(r'/org/bluez/hci[0-9]+/dev_[0-9A-Fa-f_]{17}',device):raise ValueError('Invalid Bluetooth device')
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    bus=dbus.SystemBus();loop=GLib.MainLoop();agent=Agent(bus,device,loop)
    manager=dbus.Interface(bus.get_object('org.bluez','/org/bluez'),'org.bluez.AgentManager1')
    remote=dbus.Interface(bus.get_object('org.bluez',device),'org.bluez.Device1')
    manager.RegisterAgent(agent.object_path,'KeyboardDisplay')
    def done(*_):emit(dict(event='paired'));loop.quit()
    def failed(error):emit(dict(error=str(error)));loop.quit()
    def input(source,condition):
        if condition & GLib.IO_HUP:
            remote.CancelPairing(reply_handler=lambda:None,error_handler=lambda e:None);loop.quit();return False
        try:
            data=json.loads(source.readline())
            if data.get('action')=='cancel':remote.CancelPairing(reply_handler=lambda:loop.quit(),error_handler=lambda e:loop.quit())
            else:agent.respond(data)
        except (ValueError,dbus.DBusException) as exc:emit(dict(error=str(exc)))
        return True
    GLib.io_add_watch(sys.stdin, GLib.PRIORITY_DEFAULT, GLib.IO_IN|GLib.IO_HUP,input)
    GLib.timeout_add_seconds(90,lambda:(remote.CancelPairing(reply_handler=lambda:None,error_handler=lambda e:None),failed('Pairing timed out'),False)[2])
    remote.Pair(reply_handler=done,error_handler=failed,timeout=90)
    try:loop.run()
    finally:
        try:manager.UnregisterAgent(agent.object_path)
        except dbus.DBusException:pass


if __name__=='__main__':
    try:main(sys.argv[1])
    except Exception as exc:emit(dict(error=str(exc)))
