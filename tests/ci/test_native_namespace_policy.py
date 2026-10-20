import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('policy',Path(__file__).with_name('native_namespace_policy.py'));policy=importlib.util.module_from_spec(spec);spec.loader.exec_module(policy)

class Policy(unittest.TestCase):
    def test_hosted_guard_rejects_personal_and_self_hosted_systems(self):
        env=dict(GITHUB_ACTIONS='true',RUNNER_ENVIRONMENT='github-hosted',RUNNER_OS='Linux');release=dict(ID='ubuntu',VERSION_ID='24.04');policy.hosted(env,release)
        for wrong in ({},dict(env,RUNNER_ENVIRONMENT='self-hosted'),dict(env,RUNNER_OS='Windows')):
            with self.assertRaises(ValueError):policy.hosted(wrong,release)
        for wrong in ({},dict(release,ID='nixos'),dict(release,VERSION_ID='26.04')):
            with self.assertRaises(ValueError):policy.hosted(env,wrong)
    def test_profile_is_exact_immutable_attachment_without_wildcards(self):
        path='/nix/store/'+'a'*32+'-util-linux-2.42.3-bin/bin/unshare';name,text=policy.profile_for(path)
        self.assertIn('"'+path+'"',text);self.assertIn('userns,',text);self.assertNotIn('*',text);self.assertEqual(policy.profile_for(path),(name,text))
        for wrong in ('/usr/bin/unshare',path+'"\n}', '/tmp/unshare',path.replace('/bin/unshare','/bin/bash')):
            with self.assertRaises(ValueError):policy.profile_for(wrong)
    def test_global_parameters_are_observed_without_writes(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            for name in policy.PARAMETERS:
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('1\n')
            self.assertEqual(policy.parameters(root),{name:'1' for name in policy.PARAMETERS});self.assertEqual([p.read_text() for p in root.rglob('*') if p.is_file()],['1\n']*4)
    def test_privileged_parser_receives_fixed_stdin_not_mutable_artifact_path(self):
        with patch.object(policy.subprocess,'run') as run:
            policy.execute('/usr/sbin/apparmor_parser','--add','synthetic reconstructed profile')
            args,kwargs=run.call_args;self.assertEqual(args[0],['sudo','--non-interactive','/usr/sbin/apparmor_parser','--add']);self.assertEqual(kwargs['input'],'synthetic reconstructed profile');self.assertTrue(kwargs['text'])
    def test_cleanup_reconciles_kernel_presence_after_interrupted_load(self):
        calls=[];states=iter([True,False]);policy.cleanup_policy('parser','name','fixed profile',True,present=lambda name:next(states),apply=lambda *args:calls.append(args));self.assertEqual(calls,[('parser','--remove','fixed profile')])
        calls=[];policy.cleanup_policy('parser','name','fixed profile',True,present=lambda name:False,apply=lambda *args:calls.append(args));self.assertEqual(calls,[])
        policy.cleanup_policy('parser','name','fixed profile',False,present=lambda name:self.fail('Untouched foreign profile queried'),apply=lambda *args:self.fail('Untouched foreign profile changed'))
        with self.assertRaises(ValueError):policy.cleanup_policy('parser','name','fixed profile',True,present=lambda name:True,apply=lambda *args:None)

if __name__=='__main__':unittest.main()
