import base64,os,subprocess,sys,tempfile,traceback,unittest
from pathlib import Path
from vault_artifacts import ArtifactSecrets,run_cli,cleanup

class VaultArtifacts(unittest.TestCase):
    def test_cleanup_failure_still_reaps_children_closes_pipes_and_audits_artifacts(self):
        class RefusedClose:
            def close(self):raise RuntimeError('Synthetic close failure')
        children=[subprocess.Popen([sys.executable,'-c','import time;time.sleep(10)'],stdout=subprocess.PIPE,start_new_session=True)for _ in range(2)]
        guard=ArtifactSecrets();guard.add('synthetic-cleanup-sentinel')
        with tempfile.TemporaryDirectory()as folder:
            path=Path(folder)/'diagnostic';path.write_text('synthetic-cleanup-sentinel')
            with self.assertRaisesRegex(RuntimeError,'https:RuntimeError,artifact-audit:ValueError'):
                cleanup(RefusedClose(),children[0],children[1:],guard,folder)
            self.assertNotIn('synthetic-cleanup-sentinel',path.read_text())
        self.assertTrue(all(child.poll()is not None and child.stdout.closed for child in children))
    def test_real_timeout_diagnostic_does_not_reflect_encoded_arguments(self):
        payload=base64.b64encode(b'{"password":"synthetic-timeout-sentinel"}').decode()
        try:run_cli([sys.executable,'-c','import time;time.sleep(1)',payload],dict(os.environ),timeout=.02)
        except RuntimeError as error:rendered=''.join(traceback.format_exception(error))
        else:self.fail('Actual sleeping process did not time out')
        self.assertNotIn(payload,rendered);self.assertNotIn('synthetic-timeout-sentinel',rendered)
        self.assertIn('exceeded its deadline',rendered)
    def test_reflected_credentials_are_removed_and_fail_acceptance(self):
        guard=ArtifactSecrets();values=['synthetic-account@example.invalid','synthetic-password-sentinel','synthetic-session-sentinel','synthetic-registration-sentinel']
        for value in values:guard.add(value)
        with tempfile.TemporaryDirectory()as folder:
            path=Path(folder)/'failure.json';path.write_text(' '.join(values))
            with self.assertRaises(ValueError):guard.audit(folder)
            self.assertFalse(any(value in path.read_text()for value in values))
            self.assertEqual(guard.audit(folder),1)
    def test_private_key_artifact_is_removed_and_fails_acceptance(self):
        with tempfile.TemporaryDirectory()as folder:
            path=Path(folder)/'failure.log';path.write_text('-----BEGIN PRIVATE KEY-----\nsynthetic material\n-----END PRIVATE KEY-----')
            with self.assertRaises(ValueError):ArtifactSecrets().audit(folder)
            self.assertNotIn('synthetic material',path.read_text())
    def test_common_redactor_handles_nested_reflection_and_clean_receipt(self):
        guard=ArtifactSecrets();guard.add('synthetic-long-sentinel');guard.add('synthetic-long')
        self.assertEqual(guard.redact('synthetic-long-sentinel'),'[disposable credential redacted]')
        with tempfile.TemporaryDirectory()as folder:
            (Path(folder)/'result.json').write_text('{"passed":true}')
            self.assertEqual(guard.audit(folder),1)
    def test_nonregular_artifact_is_refused_without_following_it(self):
        with tempfile.TemporaryDirectory()as folder:
            (Path(folder)/'link').symlink_to('/nonexistent')
            with self.assertRaises(ValueError):ArtifactSecrets().audit(folder)

if __name__=='__main__':unittest.main()
