import importlib.util
from pathlib import Path
import tempfile
import unittest
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

if __name__=='__main__':unittest.main()
