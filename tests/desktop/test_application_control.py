"""Force Quit uses consumed confirmation and pidfds; tests never signal live apps."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import subprocess
import signal
import unittest

SOURCE=Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime/application_control.py'
sys.path.insert(0,str(SOURCE.parent))
spec=importlib.util.spec_from_file_location('application_control',SOURCE)
control=importlib.util.module_from_spec(spec);spec.loader.exec_module(control)

class FakeBackend:
    def __init__(self):
        self.epoch='session-one';self.nodes=[dict(id=11,pid=123,app_id='fixture',name='Document',shell='xdg_shell')]
        self.identities={123:dict(pid=123,start=100,device=8,inode=9)}
        self.opened=[];self.closed=[];self.signalled=[]
    def tree(self):return self.epoch,self.nodes
    def identity(self,pid):return self.identities[pid].copy()
    def open(self,pid):self.opened.append(pid);return pid+1000
    def close(self,fd):self.closed.append(fd)
    def terminate(self,fd):self.signalled.append(fd)

class ApplicationControlTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.backend=FakeBackend();self.clock=10.
        self.control=control.ApplicationControl(self.temp.name,self.backend,lambda:self.clock)
    def prepare(self):return self.control.prepare([dict(appId='fixture',title='Document')])['token']
    def test_confirm_exact_identity_once(self):
        token=self.prepare();self.assertEqual((Path(self.temp.name)/token).stat().st_mode&0o777,0o600)
        self.assertEqual(self.control.confirm(token),dict(terminationRequested=1,alreadyExited=0))
        self.assertEqual(self.backend.signalled,[1123]);self.assertEqual(self.backend.closed,[1123])
        with self.assertRaises(FileNotFoundError):self.control.confirm(token)
    def test_pid_reuse_or_exec_change_never_signals(self):
        for key in ['start','inode','device']:
            with self.subTest(key=key):
                token=self.prepare();self.backend.identities[123][key]+=1
                with self.assertRaisesRegex(ValueError,'identity changed'):self.control.confirm(token)
                self.assertFalse(self.backend.signalled);self.assertEqual(self.backend.closed[-1],1123)
    def test_all_processes_validated_before_any_signal(self):
        self.backend.nodes.append(dict(id=12,pid=456,app_id='fixture',name='Second',shell='xdg_shell'))
        self.backend.identities[456]=dict(pid=456,start=200,device=8,inode=10)
        token=self.control.prepare([dict(appId='fixture',title='Document'),dict(appId='fixture',title='Second')])['token']
        self.backend.identities[456]['start']+=1
        with self.assertRaises(ValueError):self.control.confirm(token)
        self.assertEqual(self.backend.signalled,[]);self.assertEqual(self.backend.closed,[1123,1456])
    def test_compositor_restart_and_replaced_window_reject(self):
        token=self.prepare();self.backend.epoch='replacement'
        with self.assertRaises(ValueError):self.control.confirm(token)
        self.backend.epoch='session-one';token=self.prepare();self.backend.nodes[0]['pid']=999
        with self.assertRaises(ValueError):self.control.confirm(token)
        self.assertFalse(self.backend.signalled)
    def test_title_change_after_confirmation_does_not_change_process_identity(self):
        token=self.prepare();self.backend.nodes[0]['name']='Saved Document'
        self.assertEqual(self.control.confirm(token)['terminationRequested'],1)
    def test_ambiguous_titles_and_client_reported_x11_pids_reject(self):
        self.backend.nodes.append(self.backend.nodes[0].copy())
        with self.assertRaisesRegex(ValueError,'ambiguous'):self.prepare()
        self.backend.nodes.pop();self.backend.nodes[0]['shell']='xwayland'
        with self.assertRaisesRegex(ValueError,'native'):self.prepare()
        self.assertFalse(self.backend.signalled)
    def test_desktop_shell_cannot_be_selected(self):
        self.backend.nodes[0]['app_id']='org.quickshell'
        with self.assertRaises(ValueError):self.control.prepare([dict(appId='org.quickshell',title='Document')])
    def test_expired_cancelled_and_malformed_confirmation_reject(self):
        token=self.prepare();self.clock+=61
        with self.assertRaisesRegex(ValueError,'expired'):self.control.confirm(token)
        self.clock=10;token=self.prepare();self.control.cancel(token)
        with self.assertRaises(FileNotFoundError):self.control.confirm(token)
        with self.assertRaises(ValueError):self.control.confirm('../somewhere')
        self.assertFalse(self.backend.signalled)
    def test_receipt_membership_tampering_rejected(self):
        token=self.prepare();path=Path(self.temp.name)/token;value=json.loads(path.read_text());value['processes'][0]['pid']=999;path.write_text(json.dumps(value))
        with self.assertRaisesRegex(ValueError,'membership'):self.control.confirm(token)
        self.assertFalse(self.backend.signalled)
    def test_symlink_receipt_not_followed(self):
        token=self.prepare();path=Path(self.temp.name)/token;original=Path(self.temp.name)/'saved';path.rename(original);path.symlink_to(original)
        with self.assertRaises(OSError):self.control.confirm(token)
        self.assertFalse(self.backend.signalled)
    def test_shared_process_and_new_unselected_windows_reject(self):
        token=self.prepare()
        self.backend.nodes.append(dict(id=12,pid=123,app_id='other-app',name='Other',shell='xdg_shell'))
        with self.assertRaisesRegex(ValueError,'other windows'):self.control.confirm(token)
        with self.assertRaisesRegex(ValueError,'unselected windows'):self.prepare()
        self.assertFalse(self.backend.signalled)
    def test_cancel_and_confirm_disposable_real_process(self):
        if not hasattr(os,'pidfd_open') or not hasattr(signal,'pidfd_send_signal'):self.skipTest('Linux pidfds required')
        child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
        real=control.Backend()
        class RealProcessBackend:
            def tree(self):return 'test-owned-session',[dict(id=11,pid=child.pid,app_id='lumen-test-child',name='Disposable Fixture',shell='xdg_shell')]
            identity=real.identity
            open=real.open
            close=real.close
            terminate=real.terminate
        try:
            controller=control.ApplicationControl(Path(self.temp.name)/'real',RealProcessBackend(),lambda:10.)
            descriptors=[dict(appId='lumen-test-child',title='Disposable Fixture')]
            cancelled=controller.prepare(descriptors)['token'];controller.cancel(cancelled)
            self.assertIsNone(child.poll(),'Cancel terminated the disposable fixture')
            confirmed=controller.prepare(descriptors)['token']
            self.assertEqual(controller.confirm(confirmed)['terminationRequested'],1)
            self.assertEqual(child.wait(timeout=5),-signal.SIGKILL)
        finally:
            if child.poll() is None:child.terminate();child.wait(timeout=5)
    def test_pidfd_open_and_close_are_read_only(self):
        if not hasattr(os,'pidfd_open'):self.skipTest('Linux pidfds required')
        backend=control.Backend();fd=backend.open(os.getpid());backend.close(fd)
    def test_own_process_identity_read_only(self):
        first=control.Backend().identity(os.getpid());second=control.Backend().identity(os.getpid())
        self.assertEqual(first,second);self.assertGreater(first['start'],0)

if __name__=='__main__':unittest.main()
