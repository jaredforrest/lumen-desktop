"""A read-only connection library around Remmina's existing profiles and engine.

Never returns credentials, rewrites profiles or makes a connection while listing.
Only an explicit connect/edit/new action starts the installed client.
"""
import configparser
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import urlsplit

SUPPORTED={'RDP','VNC','SSH','SFTP','SPICE','WWW'}

def roots():
    home=Path.home()
    return (Path(os.environ.get('XDG_DATA_HOME',home/'.local/share'))/'remmina',
            Path(os.environ.get('XDG_CONFIG_HOME',home/'.config'))/'remmina')

def profiles(directories=None):
    rows=[];files={}
    for directory in (roots() if directories is None else directories):
        directory=Path(directory).resolve()
        if not directory.is_dir():continue
        for path in sorted(directory.glob('*.remmina'))[:500]:
            try:
                resolved=path.resolve()
                if not resolved.is_relative_to(directory) or not resolved.is_file() or resolved.stat().st_size>1024*1024:continue
                parser=configparser.ConfigParser(interpolation=None);parser.read(resolved,encoding='utf-8');item=parser['remmina']
                protocol=item.get('protocol','').upper()
                if protocol not in SUPPORTED:continue
                identifier=hashlib.sha256(str(resolved).encode()).hexdigest()[:24]
                if identifier in files:continue
                # A server URI may contain credentials. The library displays no URI/user information.
                server=item.get('server','')
                if '://' in server or '@' in server:server=''
                if not re.fullmatch(r'[A-Za-z0-9_.:\[\]-]{0,253}',server):server=''
                name=item.get('name','').strip() or 'Saved '+protocol+' connection'
                rows.append(dict(id=identifier,name=name[:160],protocol=protocol,host=server,
                                 printer=item.get('shareprinter','0').lower() in ('1','true')))
                files[identifier]=resolved
            except (OSError,UnicodeError,configparser.Error,KeyError):continue
    return sorted(rows,key=lambda r:r['name'].casefold()),files


def quick_uri(address,protocol):
    if protocol not in ('RDP','VNC'):raise ValueError('Choose RDP or VNC')
    if not isinstance(address,str) or not address or len(address)>253 or any(c.isspace() for c in address):raise ValueError('Enter a hostname or IP address, with an optional port')
    parsed=urlsplit(protocol.lower()+'://'+address)
    try:port=parsed.port
    except ValueError:raise ValueError('Enter a valid port') from None
    if not parsed.hostname or parsed.username is not None or parsed.password is not None or parsed.path or parsed.query or parsed.fragment:raise ValueError('Enter only a host and optional port; credentials are requested by the connection app')
    host=parsed.hostname
    if ':' in host:
        import ipaddress
        try:host='['+str(ipaddress.IPv6Address(host))+']'
        except ValueError:raise ValueError('Enter a valid IPv6 address') from None
    else:
        try:host=host.encode('idna').decode('ascii')
        except UnicodeError:raise ValueError('Enter a valid hostname') from None
        if not re.fullmatch(r'[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?',host):raise ValueError('Enter a valid hostname')
    return protocol.lower()+'://'+host+(':'+str(port) if port is not None else '')


def handle(request,directories=None,launch=None):
    action=request.get('action','list');rows,files=profiles(directories)
    if action=='list':return dict(connections=rows)
    if action in ('connect','edit'):
        path=files.get(request.get('id'))
        if path is None:raise ValueError('This saved connection is no longer available')
        # Use the original profile unchanged, including printing, keyring and SSH settings.
        command=['remmina','--disable-news','--disable-stats','--connect' if action=='connect' else '--edit',str(path)]
    elif action=='quick':command=['remmina','--disable-news','--disable-stats','--connect',quick_uri(request.get('address','').strip(),request.get('protocol','RDP'))]
    elif action=='new':command=['remmina','--disable-news','--disable-stats','--new']
    elif action=='advanced':command=['remmina','--disable-news','--disable-stats']
    else:raise ValueError('Unknown connection action')
    if launch is None:launch=lambda args:subprocess.Popen(args,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
    launch(command)
    return dict(started=True)

if __name__=='__main__':
    try:
        request=json.loads(sys.stdin.readline());print(json.dumps(handle(request)))
    except (OSError,ValueError,TypeError) as exc:
        print(json.dumps(dict(error=str(exc))));sys.exit(1)
