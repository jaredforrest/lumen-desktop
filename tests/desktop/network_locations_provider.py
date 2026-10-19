"""Disposable anonymous DAV provider smoke test on a private bus and loopback."""
import argparse,http.server,importlib.util,json,os,pathlib,subprocess,sys,tempfile,threading

class Dav(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def respond(self,code=200,body=b'',**headers):
        self.send_response(code)
        for key,value in headers.items():self.send_header(key,value)
        self.send_header('Content-Length',str(len(body)));self.end_headers()
        if self.command!='HEAD':self.wfile.write(body)
    def do_OPTIONS(self):self.respond(DAV='1,2',Allow='OPTIONS,HEAD,GET,PROPFIND')
    def do_HEAD(self):self.respond()
    def do_GET(self):self.respond(body=b'disposable fixture')
    def do_PROPFIND(self):
        length=int(self.headers.get('Content-Length','0'));self.rfile.read(length)
        entries=[('/',True)]
        if self.headers.get('Depth')!='0':entries.append(('/fixture.txt',False))
        body='<?xml version="1.0"?><d:multistatus xmlns:d="DAV:">'
        for name,directory in entries:
            body+='<d:response><d:href>'+name+'</d:href><d:propstat><d:prop><d:resourcetype>'+('<d:collection/>' if directory else '')+'</d:resourcetype><d:getcontentlength>18</d:getcontentlength><d:getlastmodified>Mon, 19 Oct 2026 10:00:00 GMT</d:getlastmodified></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>'
        body+='</d:multistatus>';self.respond(207,body.encode(),**{'Content-Type':'application/xml'})

def inside(uri):
    from gi.repository import Gio,GLib
    source=pathlib.Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell/network_locations.py'
    spec=importlib.util.spec_from_file_location('network_locations',source);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    loop=GLib.MainLoop();result=[];backend=None;connection=''
    def emit(event):
        nonlocal connection
        if event.get('id')==1:
            if event.get('error'):result.append(event);loop.quit();return
            connection=event['connectionId'];backend.dispatch({'id':2,'action':'browse','connectionId':connection})
        elif event.get('id')==2:
            result.append(event);backend.dispatch({'id':3,'action':'disconnect','connectionId':connection})
        elif event.get('id')==3:result.append(event);loop.quit()
    backend=module.Backend(emit,operation_factory=Gio.MountOperation.new)
    if 'dav' not in backend.protocols():raise RuntimeError('DAV provider unavailable in private fixture')
    backend.dispatch({'id':1,'action':'connect','uri':uri})
    GLib.timeout_add_seconds(20,lambda:(result.append({'error':'fixture timeout'}),loop.quit(),False)[-1]);loop.run();backend.close()
    assert len(result)==2 and not any(value.get('error') for value in result),result
    assert any(row['name']=='fixture.txt' for row in result[0]['rows']),result
    assert result[1].get('disconnected'),result
    print(json.dumps({'realDavConnectBrowseDisconnect':True,'credentialCollection':'none; anonymous loopback fixture','rows':len(result[0]['rows'])}))

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--provider');parser.add_argument('--inside');args=parser.parse_args()
    if args.inside:inside(args.inside)
    else:
        if not args.provider:parser.error('--provider GVFS_PACKAGE required')
        with tempfile.TemporaryDirectory(prefix='lumen-network-provider-') as temporary:
            root=pathlib.Path(temporary);server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Dav);threading.Thread(target=server.serve_forever,daemon=True).start()
            env=os.environ.copy();env.update(HOME=temporary,XDG_CONFIG_HOME=temporary,XDG_DATA_HOME=temporary,XDG_CACHE_HOME=temporary,XDG_STATE_HOME=temporary,XDG_RUNTIME_DIR=temporary,GIO_EXTRA_MODULES=args.provider+'/lib/gio/modules',GVFS_DISABLE_FUSE='1',XDG_DATA_DIRS=args.provider+'/share',GIO_USE_VFS='gvfs')
            # Private D-Bus configuration sees only the chosen provider's services.
            config=root/'bus.conf';config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+temporary+'</listen><auth>EXTERNAL</auth><servicedir>'+args.provider+'/share/dbus-1/services</servicedir><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
            try:subprocess.run(['dbus-run-session','--config-file='+str(config),'--',sys.executable,__file__,'--inside','dav://127.0.0.1:'+str(server.server_port)+'/'],env=env,check=True,timeout=30)
            finally:server.shutdown();server.server_close()
