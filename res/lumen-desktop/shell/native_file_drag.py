"""Observe compositor drag facts without file paths or input synthesis."""
import json, os, socket, struct

class DragState:
    def __init__(self, seat='seat0'):
        self.seat, self.serial, self.active = seat, -1, False
    def update(self, value):
        if not isinstance(value, dict) or value.get('seat') != self.seat or (type(value.get('version')) is not int or value.get('version') != 1):
            return False
        serial, active = value.get('serial'), value.get('active')
        if type(serial) is not int or serial < 0 or type(active) is not bool or serial <= self.serial:
            return False
        self.serial, self.active = serial, active
        return True
    def refresh(self, seats):
        if not isinstance(seats,list):return False
        for seat in seats:
            if isinstance(seat,dict) and seat.get('name')==self.seat and isinstance(seat.get('lumen_drag'),dict):
                value=dict(seat['lumen_drag'],seat=self.seat)
                serial=value.get('serial')
                if type(serial) is not int or serial<0 or type(value.get('active')) is not bool or type(value.get('version')) is not int or value.get('version')!=1:return False
                self.update(value)
                return True
        return False
    def disconnect(self):
        self.serial, self.active = -1, False


def receive(stream):
    def read(size):
        data = bytearray()
        while len(data) < size:
            part = stream.recv(size-len(data))
            if not part: raise EOFError('Compositor disconnected')
            data.extend(part)
        return data
    header = read(14)
    if header[:6] != b'i3-ipc': raise ValueError('Invalid IPC header')
    size, kind = struct.unpack('<II', header[6:])
    if size > 1048576: raise ValueError('Oversized IPC response')
    return kind, json.loads(read(size))


def send(stream, kind, value=''):
    data=value.encode();stream.sendall(b'i3-ipc'+struct.pack('<II',len(data),kind)+data)


def main():
    state=DragState(os.environ.get('LUMEN_DESKTOP_SEAT','seat0'))
    def publish(): print(json.dumps({'active':state.active,'available':state.serial>=0}),flush=True)
    publish()
    try:
        address=os.environ['SWAYSOCK']
        with socket.socket(socket.AF_UNIX) as events, socket.socket(socket.AF_UNIX) as query:
            events.settimeout(5);events.connect(address);send(events,2,'["tick"]')
            _,ack=receive(events)
            if not isinstance(ack,dict) or ack.get('success') is not True:return
            query.settimeout(5);query.connect(address);send(query,7);_,version=receive(query)
            if not isinstance(version,dict) or version.get('native_file_drag_state') is not True:return
            def refresh():
                send(query,101);_,seats=receive(query)
                return state.refresh(seats)
            if not refresh():return
            publish();events.settimeout(None)
            while True:
                kind,value=receive(events)
                if kind != 0x80000007 or not isinstance(value,dict):continue
                try:payload=json.loads(value.get('payload',''))
                except (ValueError,TypeError):continue
                # send_tick is public IPC: its payload is an invalidation only.
                # Always re-read compositor-owned seats; never accept its serial.
                if isinstance(payload,dict) and payload.get('kind')=='lumen-drag':
                    if not refresh():return
                    publish()
    except (OSError,ValueError,KeyError,EOFError,TypeError):pass
    finally:state.disconnect();publish()

if __name__=='__main__':main()
