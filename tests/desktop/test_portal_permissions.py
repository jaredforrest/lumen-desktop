"""Real private D-Bus protocol acceptance; never use the account's permission bus."""
import importlib.util,json,os,subprocess,sys,tempfile,time,unittest
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
source=ROOT/'res/lumen-desktop/shell/portal_permissions.py'
spec=importlib.util.spec_from_file_location('portal_permissions',source);portal=importlib.util.module_from_spec(spec);spec.loader.exec_module(portal)


class PortalTests(unittest.TestCase):
    def test_private_provider_protocol_and_exact_target(self):
        if os.environ.get('LUMEN_PRIVATE_PERMISSION_TEST')!='1':
            with tempfile.TemporaryDirectory(prefix='lumen-private-permissions-') as private:
                env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,LUMEN_PRIVATE_PERMISSION_TEST='1')
                for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=private+'/'+key.lower()
                for key in ('DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS','DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(key,None)
                config=Path(private)/'bus.conf';config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
                result=subprocess.run(['dbus-run-session','--config-file='+str(config),'--',sys.executable,__file__],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=35)
                self.assertEqual(result.returncode,0,result.stdout)
            return
        import dbus
        from dbus.mainloop.glib import DBusGMainLoop
        from gi.repository import GLib
        DBusGMainLoop(set_as_default=True)
        with tempfile.TemporaryDirectory() as directory:
            ready=Path(directory)/'ready'
            process=subprocess.Popen([sys.executable,str(ROOT/'tests/desktop/fixtures/portal_permission_mock.py'),'--ready',str(ready)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            try:
                end=time.monotonic()+5
                while not ready.exists() and time.monotonic()<end:
                    if process.poll() is not None:self.fail(process.communicate()[1])
                    time.sleep(.03)
                self.assertTrue(ready.exists(),'Private provider did not start')
                bus=dbus.SessionBus();obj=bus.get_object(portal.NAME,portal.PATH,introspect=False)
                admin=dbus.Interface(obj,'org.example.PermissionFixture')
                events=[]
                receiver=bus.add_signal_receiver(lambda table,resource,*rest:events.append((str(table),str(resource))),signal_name='Changed',dbus_interface=portal.INTERFACE)
                def snapshot():return portal.handle({'action':'list'})
                def row(capability,app='org.example.First'):return next(item for item in snapshot()['rows'] if item['capability']==capability and item['appId']==app)
                def action(item,kind,confirmed=True):return portal.handle(dict(action=kind,id=item['id'],revision=item['revision'],confirmed=confirmed))
                initial=snapshot();self.assertTrue(initial['available']);self.assertEqual(initial['version'],2)
                self.assertEqual(len(initial['rows']),8)
                encoded=json.dumps(initial)
                for secret in ('private-restore-resource','private-remote-resource','PRIVATE_PROVIDER_METADATA_DO_NOT_EXPORT'):
                    self.assertNotIn(secret,encoded)
                self.assertNotIn('microphone',encoded.lower().replace(initial['scope'].lower(),''))
                unidentified=next(item for item in initial['rows'] if not item['appId']);self.assertEqual(unidentified['actions'],[])
                self.assertEqual(row('Screenshots','org.example.Unknown')['actions'],[])
                camera=row('Camera');self.assertIn('confirmation',action(camera,'deny',False)['error'])
                before=json.loads(admin.Dump())['tables']
                changed=action(camera,'deny');self.assertTrue(changed['available'],changed);self.assertIn('Existing capture may continue',changed['message'])
                after=json.loads(admin.Dump());expected=json.loads(json.dumps(before));expected['devices']['camera']['org.example.First']=['no'];self.assertEqual(after['tables'],expected)
                self.assertEqual(after['calls'][-1],['SetPermission','devices','camera','org.example.First',['no']])
                end=time.monotonic()+2
                while ('devices','camera') not in events and time.monotonic()<end:GLib.MainContext.default().iteration(False);time.sleep(.01)
                self.assertIn(('devices','camera'),events)
                self.assertIn('changed',action(camera,'reset')['error'])
                camera=row('Camera');changed=action(camera,'reset');self.assertTrue(changed['available'],changed)
                self.assertEqual(json.loads(admin.Dump())['calls'][-1],['DeletePermission','devices','camera','org.example.First'])
                saved=row('Saved screen sharing');saved_id=saved['id'];changed=action(saved,'forget');self.assertTrue(changed['available'],changed)
                self.assertNotIn(saved_id,[item['id'] for item in changed['rows']]);self.assertEqual(len([item for item in changed['rows'] if item['capability']=='Saved screen sharing']),1)
                # Unknown method versions stay read-only for deletion; deny is v1.
                admin.Configure(1,False,'');self.assertNotIn('forget',row('Saved remote desktop','org.example.Second')['actions'])
                admin.Configure(2,True,'');item=row('Screenshots');self.assertIn('did not confirm',action(item,'deny')['error'])
                admin.Configure(2,False,'screenshot');partial=snapshot();self.assertTrue(partial['available']);self.assertTrue(any(item['state']=='unavailable' for item in partial['coverage']));self.assertFalse(any(item['capability']=='Screenshots' for item in partial['rows']))
                admin.Configure(2,False,'');self.assertTrue(snapshot()['available'])
                # CLI transport consumes one JSON line and redacts provider data.
                cli=subprocess.run([sys.executable,str(source)],input='{"action":"list"}\n',text=True,capture_output=True,check=True)
                self.assertTrue(json.loads(cli.stdout)['available']);self.assertNotIn('private-restore-resource',cli.stdout)
                stale=row('Screenshots');receiver.remove()
                process.terminate();_,stderr=process.communicate(timeout=5);self.assertEqual(stderr,'')
                ready.unlink();process=subprocess.Popen([sys.executable,str(ROOT/'tests/desktop/fixtures/portal_permission_mock.py'),'--ready',str(ready)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
                end=time.monotonic()+5
                while not ready.exists() and time.monotonic()<end:time.sleep(.03)
                self.assertTrue(ready.exists());self.assertIn('changed',action(stale,'deny')['error'])

            finally:
                process.terminate();stdout,stderr=process.communicate(timeout=5)
                self.assertEqual(stderr,'',stderr)
            unavailable=portal.handle({'action':'list'});self.assertFalse(unavailable['available']);self.assertIn('unavailable',unavailable['error']);self.assertEqual(unavailable['rows'],[])


if __name__=='__main__':unittest.main()
