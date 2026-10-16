import importlib.util,json,os,subprocess,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('settings',ROOT/'res/lumen-desktop/shell/nix_settings.py');settings=importlib.util.module_from_spec(spec);spec.loader.exec_module(settings)
class GitDeploymentTest(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.base=Path(self.tmp.name);self.repo=self.base/'repo';self.repo.mkdir();(self.repo/'settings').mkdir();self.source=self.repo/'settings/desktop.nix';self.source.write_text(settings.serialize({'appearance':{'mode':'light'}}));(self.repo/'.gitignore').write_text('settings/.desktop-settings-*.nix\n')
  self.git('init','-b','main');self.git('config','user.name','Test');self.git('config','user.email','test@example.invalid');self.git('add','.');self.git('commit','-m','Baseline')
  self.initial=self.git('rev-parse','HEAD');self.original=self.source.read_bytes();self.bundle=self.base/'bundle';self.bundle.mkdir()
  self.env=patch.dict(os.environ,{'LUMEN_NIX_SETTINGS_SOURCE':str(self.source),'LUMEN_NIX_SETTINGS_BUILD_SOURCE':str(self.repo),'LUMEN_NIX_SETTINGS_DEPLOY':'/fake/deploy','LUMEN_NIX_SETTINGS_PKEXEC':'/fake/pkexec','XDG_RUNTIME_DIR':str(self.base/'runtime')});self.env.start()
 def tearDown(self):self.env.stop();self.tmp.cleanup()
 def git(self,*args):return subprocess.check_output(['git','-C',str(self.repo),*args],text=True,stderr=subprocess.DEVNULL).strip()
 def build(self,path):
  (self.bundle/'settings.json').write_text(json.dumps({'appearance':{'mode':'dark' if 'dark' in path.read_text() else 'light'}}));return self.bundle
 def test_applied_reads_do_not_wait_for_the_edit_lock(self):
  with patch.object(settings,'runtime',side_effect=AssertionError('Read attempted to take the edit lock')),patch.object(settings,'read_settings',return_value={'applied':True}):
   self.assertEqual(settings.transaction(),{'applied':True})
 def test_settings_are_committed_before_system_build(self):
  real=settings.subprocess.run
  def execute(cmd,**kwargs):
   if cmd[0] in ['/fake/pkexec','/fake/deploy']:return subprocess.CompletedProcess(cmd,0,stdout='')
   return real(cmd,**kwargs)
  def rebuild(candidate):
   self.assertNotEqual(self.git('rev-parse','HEAD'),self.initial);self.assertEqual(self.git('status','--porcelain'),'');self.assertIn('dark',self.git('show','HEAD:settings/desktop.nix'));return Path('/nix/store/test-only-system')
  with patch.object(settings,'build',side_effect=self.build),patch.object(settings,'rebuild_system',side_effect=rebuild),patch.object(settings.subprocess,'run',side_effect=execute):
   settings.transaction('appearance',{'mode':'dark'})
  self.assertEqual(self.git('status','--porcelain'),'')
 def test_failed_build_creates_clean_rollback_commit(self):
  with patch.object(settings,'build',side_effect=self.build),patch.object(settings,'rebuild_system',side_effect=ValueError('Build failed')):
   with self.assertRaises(ValueError):settings.transaction('appearance',{'mode':'dark'})
  self.assertEqual(self.source.read_bytes(),self.original);self.assertEqual(self.git('status','--porcelain'),'');self.assertEqual(self.git('rev-list','--count','HEAD'),'3')
 def test_pending_edits_are_not_silently_deployed(self):
  (self.repo/'pending.nix').write_text('{}')
  with self.assertRaisesRegex(ValueError,'pending'):settings.transaction('appearance',{'mode':'dark'})
  self.assertEqual(self.git('rev-parse','HEAD'),self.initial);self.assertEqual(self.source.read_bytes(),self.original)
if __name__=='__main__':unittest.main()
