"""Dispatch a media key to a real, capable session player; empty sessions are normal."""
import sys
import dbus

PATH='/org/mpris/MediaPlayer2'
PLAYER='org.mpris.MediaPlayer2.Player'


def dispatch(bus, action):
    methods={'play-pause':('PlayPause',None),'next':('Next','CanGoNext'),'previous':('Previous','CanGoPrevious')}
    if action not in methods:raise ValueError('Unknown media action')
    candidates=[]
    for name in sorted(str(n) for n in bus.list_names() if str(n).startswith('org.mpris.MediaPlayer2.') and str(n)!='org.mpris.MediaPlayer2.playerctld'):
        try:
            obj=bus.get_object(name,PATH)
            properties=dbus.Interface(obj,'org.freedesktop.DBus.Properties').GetAll(PLAYER,timeout=2)
            if not properties.get('CanControl',False):continue
            method,capability=methods[action]
            if capability and not properties.get(capability,False):continue
            if action=='play-pause' and not properties.get('CanPause' if properties.get('PlaybackStatus')=='Playing' else 'CanPlay',False):continue
            candidates.append((properties.get('PlaybackStatus')!='Playing',name,obj,method))
        except dbus.DBusException as exc:
            if exc.get_dbus_name() not in ('org.freedesktop.DBus.Error.ServiceUnknown','org.freedesktop.DBus.Error.NameHasNoOwner','org.freedesktop.DBus.Error.UnknownObject'):raise
    for _,name,obj,method in sorted(candidates,key=lambda item:(item[0],item[1])):
        try:
            getattr(dbus.Interface(obj,PLAYER),method)(timeout=2)
            return True
        except dbus.DBusException as exc:
            if exc.get_dbus_name() not in ('org.freedesktop.DBus.Error.ServiceUnknown','org.freedesktop.DBus.Error.NameHasNoOwner','org.freedesktop.DBus.Error.UnknownObject'):raise
    return False


def main():
    if len(sys.argv)!=2 or sys.argv[1] not in ('play-pause','next','previous'):raise SystemExit('Usage: lumen-media-key play-pause|next|previous')
    try:dispatch(dbus.SessionBus(),sys.argv[1])
    except dbus.DBusException as exc:
        print('Media command failed: '+str(exc),file=sys.stderr)
        raise SystemExit(1)


if __name__=='__main__':main()
