"""Small read/command boundary; never reads network credentials or writes Nix files."""
import json, os, re, socket, subprocess, sys, tempfile, time, fcntl
import dbus
from pathlib import Path
import bluetooth_power
import battery_power
import network_connectivity

class Wireless:
    def __init__(self):
        sockets = sorted(Path("/run/wpa_supplicant").glob("*"))
        self.path = next((p for p in sockets if p.is_socket() and p.name != "global"), None)
    def request(self, command):
        if self.path is None:
            raise RuntimeError("Wi-Fi service unavailable")
        with tempfile.TemporaryDirectory(prefix="lumen-wifi-", dir=os.environ.get("XDG_RUNTIME_DIR")) as directory:
            with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as client:
                client.settimeout(3)
                client.bind(str(Path(directory)/"control"))
                client.connect(str(self.path))
                client.send(command.encode())
                result = client.recv(65536).decode("utf-8", errors="replace").strip()
                if result.startswith(("FAIL", "UNKNOWN")):
                    raise RuntimeError("Wi-Fi request failed")
                return result
    def status(self):
        if self.path is None:
            return {"available": False, "networks": []}
        status = dict(line.split("=", 1) for line in self.request("STATUS").splitlines() if "=" in line)
        networks = []
        for line in self.request("LIST_NETWORKS").splitlines()[1:]:
            fields = line.split("\t")
            if len(fields) >= 4:
                networks.append({"id": fields[0], "name": fields[1], "connected": "[CURRENT]" in fields[3] and status.get("wpa_state") == "COMPLETED"})
        return {"available": True, "state": status.get("wpa_state", "UNKNOWN"), "name": status.get("ssid", ""), "networks": networks}

def command(args):
    result = subprocess.run(args, capture_output=True, text=True, timeout=12)
    if result.returncode:
        raise RuntimeError("The service rejected the request")
    return result.stdout.strip()

def main():
    action = sys.argv[1] if len(sys.argv) > 1 else "status"
    wifi = Wireless()
    if action in ("bluetooth-on", "bluetooth-off", "bluetooth-connect", "bluetooth-disconnect"):
        path = sys.argv[2]
        if not re.fullmatch(r"/org/bluez/hci[0-9]+(?:/dev_[0-9A-Fa-f_]{17})?", path):
            raise RuntimeError("Invalid Bluetooth device")
        obj = dbus.SystemBus().get_object("org.bluez", path)
        if action in ("bluetooth-on", "bluetooth-off"):
            if action == "bluetooth-on":
                bluetooth_power.prepare_enable(path)
            dbus.Interface(obj, "org.freedesktop.DBus.Properties").Set("org.bluez.Adapter1", "Powered", dbus.Boolean(action == "bluetooth-on"), timeout=12)
        else:
            device = dbus.Interface(obj, "org.bluez.Device1")
            (device.Connect if action == "bluetooth-connect" else device.Disconnect)(timeout=12)
        return {"ok":True}
    if action == "displays":
        cache = Path(os.environ.get('XDG_RUNTIME_DIR','/tmp'))/('lumen-ddc-'+str(os.getuid())+'.json')
        with cache.with_suffix('.lock').open('a') as lock:
            os.chmod(lock.name,0o600);fcntl.flock(lock,fcntl.LOCK_EX)
            try:state=json.loads(cache.read_text())
            except (OSError,ValueError):state={}
            if not isinstance(state,dict) or not isinstance(state.get('devices'),list) or not all(isinstance(d,dict) and str(d.get('bus','')).isdigit() for d in state.get('devices',[])) or not all(isinstance(state.get(k,0),(int,float)) for k in ('detected','read')):state={}
            topology=command(['swaymsg','-r','-t','get_outputs']) if os.environ.get('SWAYSOCK') else ''
            # Exclude brightness itself; identity/topology changes invalidate detection.
            try:identity=json.dumps([(o['name'],o.get('make'),o.get('model'),o.get('serial'),o.get('active')) for o in json.loads(topology)])
            except ValueError:identity=''
            now=time.clock_gettime(time.CLOCK_BOOTTIME)
            if state.get('identity')!=identity or now-state.get('detected',0)>300:
                detected=command(['ddcutil','detect','--brief']);devices=[]
                for block in re.split(r'(?m)^Display ',detected)[1:]:
                    bus=re.search(r'I2C bus:\s*/dev/i2c-(\d+)',block);label=re.search(r'Monitor:\s*(.+)',block)
                    if bus:devices.append(dict(bus=bus[1],name=label[1] if label else 'Display'))
                state=dict(identity=identity,detected=now,devices=devices,read=0)
            if now-state.get('read',0)>2:
                displays=[]
                for device in state['devices']:
                    try:
                        value=command(['ddcutil','--bus',device['bus'],'getvcp','10','--brief'])
                        match=re.search(r'VCP 10 C (\d+) (\d+)',value)
                        if match and int(match[2])>0:displays.append(dict(device,level=int(match[1])/int(match[2]),maximum=int(match[2])))
                    except (OSError,RuntimeError,subprocess.TimeoutExpired):
                        state['detected']=0
                state.update(displays=displays,read=now)
                cache.write_text(json.dumps(state));cache.chmod(0o600)
            return {'displays':state.get('displays',[])}
    if action == "brightness":
        bus, value = sys.argv[2:4]
        if not bus.isdigit() or not value.isdigit() or not 0 <= int(value) <= 65535:
            raise RuntimeError("Invalid brightness request")
        command(["ddcutil", "--bus", bus, "setvcp", "10", value])
        cache=Path(os.environ.get('XDG_RUNTIME_DIR','/tmp'))/('lumen-ddc-'+str(os.getuid())+'.json')
        try:
            with cache.with_suffix('.lock').open('a') as lock:
                fcntl.flock(lock,fcntl.LOCK_EX)
                state=json.loads(cache.read_text());state['read']=0;cache.write_text(json.dumps(state))
        except (OSError,ValueError,TypeError):pass
        return {"ok":True}
    if action == "status":
        try:
            wireless = wifi.status()
        except (OSError, RuntimeError):
            wireless = {"available": False, "networks": []}
        wireless=network_connectivity.merge(wireless,network_connectivity.status(wifi.path.name if wifi.path else None))
        return {"wifi": wireless, "bluetoothRadios": bluetooth_power.status(), "power": battery_power.status()}
    if action == "wifi-connect":
        ident = sys.argv[2]
        if not ident.isdigit() or ident not in {n["id"] for n in wifi.status()["networks"]}:
            raise RuntimeError("Network is no longer configured")
        wifi.request("SELECT_NETWORK " + ident)
    elif action in ("wifi-disconnect", "wifi-reconnect"):
        wifi.request("DISCONNECT" if action.endswith("disconnect") else "RECONNECT")
    else:
        raise RuntimeError("Unknown control")
    return {"ok": True}

if __name__ == "__main__":
    try:
        print(json.dumps(main(), ensure_ascii=False))
    except dbus.DBusException:
        print(json.dumps({"error":"Bluetooth could not complete the request. Check that the device is on and in range."}))
        sys.exit(1)
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
        print(json.dumps({"error": str(error)}))
        sys.exit(1)
