"""Private Sway IPC protocol provider and disposable persistence adapter."""
import copy,json,os,shlex,socket,struct,sys,threading
from pathlib import Path
MAGIC=b'i3-ipc'
DEVICES=[
 dict(identifier='1:1:Fixture_Keyboard',name='Fixture Keyboard',type='keyboard',repeat_rate=25,repeat_delay=600),
 dict(identifier='2:2:Fixture_Mouse',name='Fixture Mouse',type='pointer',scroll_factor=1.,libinput=dict(accel_speed=0.,natural_scroll='disabled',left_handed='disabled')),
 dict(identifier='3:3:Fixture_Trackpad',name='Fixture Trackpad',type='touchpad',scroll_factor=1.,libinput=dict(accel_speed=.2,natural_scroll='enabled',left_handed='disabled',tap='disabled',dwt='enabled')),
]


def settings_adapter(root,base_path=None):
    file=root/'settings.json';file.write_text('{"inputs":{}}')
    binary=root/'bin';binary.mkdir();script=binary/'lumen-nix-settings'
    script.write_text('#!'+sys.executable+'\n'+'''import json,os,sys
from pathlib import Path
file=Path(os.environ['LUMEN_NIX_SETTINGS_ACTIVE'])
if file.with_suffix('.deny').exists():print(json.dumps({'error':'Fixture persistence refused'}));sys.exit(1)
values=json.loads(file.read_text());values[sys.argv[2]]=json.loads(sys.argv[3]);pending=file.with_suffix('.new');pending.write_text(json.dumps(values));pending.replace(file);print(json.dumps(values))
''');script.chmod(0o755)
    return dict(LUMEN_NIX_SETTINGS_ACTIVE=str(file),PATH=str(binary)+os.pathsep+(base_path if base_path is not None else os.environ.get('PATH','')))


class InputServer:
    def __init__(self,path):
        self.path=str(path);self.devices=copy.deepcopy(DEVICES);self.actions=[];self.subscribers=[];self.connections=[];self.ignore=False;self.reject=False
        self.socket=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);self.socket.bind(self.path);self.socket.listen();self.closed=False
        self.thread=threading.Thread(target=self.accept,daemon=True);self.thread.start()

    def accept(self):
        while not self.closed:
            try:connection,_=self.socket.accept()
            except OSError:return
            self.connections.append(connection);threading.Thread(target=self.client,args=(connection,),daemon=True).start()

    def packet(self,connection,kind,value):
        data=json.dumps(value).encode();connection.sendall(struct.pack('<6sII',MAGIC,len(data),kind)+data)

    def receive(self,connection,count):
        data=b''
        while len(data)<count:
            chunk=connection.recv(count-len(data))
            if not chunk:raise OSError('Closed')
            data+=chunk
        return data

    def client(self,connection):
        try:
            while True:
                magic,length,kind=struct.unpack('<6sII',self.receive(connection,14));assert magic==MAGIC
                payload=self.receive(connection,length).decode()
                if kind==100:result=copy.deepcopy(self.devices)
                elif kind==2:
                    assert json.loads(payload)==['input'];self.subscribers.append(connection);result={'success':True}
                elif kind==0:
                    tokens=shlex.split(payload);assert len(tokens)==4 and tokens[0]=='input';_,identifier,key,value=tokens
                    assert identifier not in ('*','type:keyboard','type:pointer','type:touchpad')
                    self.actions.append(tokens[1:]);result=[dict(success=not self.reject)]
                    if not self.reject and not self.ignore:
                        for device in self.devices:
                            if device['identifier']!=identifier:continue
                            if key in ('repeat_rate','repeat_delay'):device[key]=int(value)
                            elif key=='scroll_factor':device[key]=float(value)
                            else:device['libinput']['accel_speed' if key=='pointer_accel' else key]=float(value) if key=='pointer_accel' else value
                else:raise AssertionError(kind)
                self.packet(connection,kind,result)
        except (OSError,ConnectionError):pass

    def add(self,device):
        self.devices.append(copy.deepcopy(device))
        for connection in self.subscribers[:]:
            try:self.packet(connection,0x80000015,dict(change='added',input=device))
            except OSError:self.subscribers.remove(connection)

    def close(self):
        self.closed=True;self.socket.close()
        for connection in self.connections:
            try:connection.shutdown(socket.SHUT_RDWR);connection.close()
            except OSError:pass
