"""Opt-in query history keeps bounded private data and clears on disable."""
import importlib.util,json,os,signal,subprocess,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'res/lumen-desktop/shell'))
import search_history as history
class SearchHistory(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
        self.environment=patch.dict(os.environ,{'XDG_STATE_HOME':str(self.root)});self.environment.start()
        self.settings=patch.object(history,'read_settings',return_value={'search':{'queryHistory':True}});self.settings.start()
    def tearDown(self):self.settings.stop();self.environment.stop();self.temp.cleanup()
    def call(self,action,**values):
        if action=='record' and 'generation' not in values:values['generation']=history.handle({'action':'list'})['generation']
        return history.handle(dict(action=action,**values))
    def test_history_is_explicit_opt_in_and_disable_removes_data(self):
        self.call('record',query='private fixture')
        path=self.root/'lumen-desktop/search/queries.json';self.assertTrue(path.exists())
        with patch.object(history,'enabled',return_value=False):
            self.assertEqual(self.call('list')['queries'],[])
            self.call('record',query='must never be stored')
        self.assertFalse(path.exists())
    def test_retired_record_after_clear_or_disable_cannot_reappear(self):
        generation=self.call('list')['generation']
        self.call('clear')
        self.assertTrue(self.call('record',query='retired',generation=generation)['retired'])
        generation=self.call('list')['generation']
        with patch.object(history,'enabled',return_value=False):self.call('list')
        self.assertTrue(self.call('record',query='revoked',generation=generation)['retired'])
        self.assertEqual(self.call('list')['queries'],[])
        self.assertTrue(history.handle({'action':'record','query':'no authority'})['retired'])
    def test_real_queued_worker_cannot_restore_cleared_generation(self):
        policy=self.root/'policy.json';policy.write_text(json.dumps({'search':{'queryHistory':True}}))
        environment=dict(os.environ,LUMEN_NIX_SETTINGS_ACTIVE=str(policy))
        def cli(data):
            return json.loads(subprocess.check_output([sys.executable,str(Path(history.__file__))],input=json.dumps(data)+'\n',text=True,env=environment))
        epoch=cli({'action':'list'})['generation']
        child=subprocess.Popen([sys.executable,str(Path(history.__file__))],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=environment)
        try:
            os.kill(child.pid,signal.SIGSTOP)
            child.stdin.write(json.dumps({'action':'record','query':'queued private fixture','generation':epoch})+'\n');child.stdin.flush()
            cleared=cli({'action':'clear'});self.assertNotEqual(cleared['generation'],epoch)
            os.kill(child.pid,signal.SIGCONT);stdout,stderr=child.communicate(timeout=5)
            self.assertEqual(child.returncode,0,stderr);response=json.loads(stdout)
            self.assertTrue(response['retired']);self.assertEqual(response['queries'],[])
            self.assertEqual(cli({'action':'list'})['queries'],[])
            self.assertEqual(cli({'action':'record','query':'new intent','generation':cleared['generation']})['queries'],['new intent'])
        finally:
            if child.poll() is None:child.kill();child.wait(timeout=3)
            for stream in (child.stdin,child.stdout,child.stderr):
                if stream:stream.close()
    def test_deduplicated_mru_bounded_and_clear(self):
        for i in range(25):self.call('record',query='query '+str(i))
        values=self.call('record',query='query 17')['queries']
        self.assertEqual(len(values),20);self.assertEqual(values[0],'query 17');self.assertEqual(values.count('query 17'),1)
        self.assertNotIn('query 0',values)
        path=self.root/'lumen-desktop/search/queries.json'
        self.assertEqual(path.stat().st_mode&0o777,0o600);self.assertEqual(path.parent.stat().st_mode&0o777,0o700)
        self.assertEqual(self.call('clear')['queries'],[]);self.assertFalse(path.exists())
    def test_invalid_input_does_not_replace_saved_history(self):
        self.call('record',query='keep')
        for query in (False,'x'*513,'line\nbreak','nul\0value'):
            with self.subTest(query=query),self.assertRaises(ValueError):self.call('record',query=query)
        self.assertEqual(self.call('list')['queries'],['keep'])
    def test_symlink_and_insecure_store_refused(self):
        self.call('record',query='keep')
        path=self.root/'lumen-desktop/search/queries.json';external=self.root/'external';external.write_text('["external"]');path.unlink();path.symlink_to(external)
        with self.assertRaises(ValueError):self.call('list')
        self.assertEqual(external.read_text(),'["external"]')
        path.unlink();path.write_text('["insecure"]');path.chmod(0o644)
        with self.assertRaises(ValueError):self.call('list')
    def test_malformed_store_is_visible_and_clear_recovers(self):
        self.call('record',query='keep');path=self.root/'lumen-desktop/search/queries.json';path.write_text('{}')
        with self.assertRaises(ValueError):self.call('list')
        self.call('clear');self.assertEqual(self.call('list')['queries'],[])
if __name__=='__main__':unittest.main()
