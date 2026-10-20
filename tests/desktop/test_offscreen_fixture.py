"""Immutable inputs and fresh artifacts for shared private Qt fixtures."""
import json,subprocess,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
from application_fixture import offscreen

class Offscreen(unittest.TestCase):
 def test_dependencies_must_be_existing_immutable_files(self):
  self.assertEqual(offscreen.immutable_file(sys.executable),Path(sys.executable).resolve())
  with tempfile.TemporaryDirectory() as directory:
   local=Path(directory)/'tool';local.write_text('synthetic')
   for value in (local,Path(sys.executable).parent,Path('/nix/store/nonexistent-fixture')):
    with self.subTest(value=str(value)),self.assertRaises(ValueError):offscreen.immutable_file(value)
 def test_wrong_interpreter_rejects_before_creating_artifacts(self):
  with tempfile.TemporaryDirectory() as directory:
   output=Path(directory)/'new'
   with self.assertRaises(ValueError):offscreen.run(directory,output,sys.executable,sys.executable,['/unavailable/interpreter'])
   self.assertFalse(output.exists())
 def test_existing_artifact_directory_is_never_reused(self):
  with tempfile.TemporaryDirectory() as directory:
   output=Path(directory)/'existing';output.mkdir();(output/'sentinel').write_text('preserved')
   with self.assertRaises(FileExistsError):offscreen.run(directory,output,sys.executable,sys.executable,[sys.executable])
   self.assertEqual((output/'sentinel').read_text(),'preserved')
 def repository(self,source):
  source.mkdir();(source/'probe.py').write_text('print("synthetic fixture")\n')
  for args in (['init','--initial-branch=main'],['add','.'],['-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-m','Fixture']):
   subprocess.run(['git','-c','core.hooksPath=/dev/null',*args],cwd=source,check=True,capture_output=True,text=True)
 def test_actual_shell_prefix_and_boundary_arguments_are_preserved(self):
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);source=root/'repo';self.repository(source);launcher=root/'launcher';font=root/'fonts.conf';font.write_text('<fontconfig/>');launcher.write_text('export QT_QUICK_BACKEND=software\nexec quickshell /immutable/fixture.qml\n');output=root/'output';command=[sys.executable,'/source/probe.py']
   def boundary(repo,out,args,env):
    self.assertEqual(repo,source);self.assertEqual(out,output);self.assertEqual(args,command);self.assertEqual(env['FONTCONFIG_FILE'],str(font));self.assertEqual(env['QT_QUICK_BACKEND'],'software');return 7
   with patch.object(offscreen,'immutable_file',side_effect=lambda value:Path(value).resolve()),patch.object(offscreen.isolation,'run',side_effect=boundary):
    self.assertEqual(offscreen.run(source,output,launcher,font,command),7)
   record=json.loads((output/'offscreen-boundary.json').read_text());self.assertEqual(record['exitCode'],7);self.assertEqual(record['command'],command)
   self.assertEqual(len(record['sourceRevision']),40)
   for key in ('sourceSha256','launcherSha256','fontConfigSha256'):self.assertEqual(len(record[key]),64)
 def test_dirty_actual_repository_rejects_before_boundary_or_artifact_write(self):
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);source=root/'repo';self.repository(source);(source/'pending').write_text('uncommitted');output=root/'output'
   with patch.object(offscreen.isolation,'run') as boundary,self.assertRaisesRegex(ValueError,'Freeze source'):
    offscreen.run(source,output,sys.executable,sys.executable,[sys.executable,'/source/probe.py'])
   boundary.assert_not_called();self.assertFalse(output.exists())
 def test_changed_source_during_boundary_preserves_initial_hash_and_rejects(self):
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);source=root/'repo';self.repository(source);output=root/'output';before=(source/'probe.py').read_bytes();launcher=root/'launcher';launcher.write_text('export QT_QUICK_BACKEND=software\nexec quickshell /immutable/fixture.qml\n')
   def boundary(*args):
    (source/'probe.py').write_text('changed during fixture');return 0
   with patch.object(offscreen,'immutable_file',side_effect=lambda value:Path(value).resolve()),patch.object(offscreen.isolation,'run',side_effect=boundary),self.assertRaisesRegex(ValueError,'source changed'):
    offscreen.run(source,output,launcher,sys.executable,[sys.executable,'/source/probe.py'])
   record=json.loads((output/'offscreen-boundary.json').read_text());self.assertFalse(record['sourceUnchanged']);self.assertEqual(record['sourceSha256'],__import__('hashlib').sha256(before).hexdigest())
 def test_inside_environment_refuses_an_ordinary_home(self):
  with patch.dict(offscreen.os.environ,{'HOME':'/ordinary-user'},clear=True),self.assertRaises(ValueError):offscreen.environment()

if __name__=='__main__':unittest.main()
