"""Run actual child commands; a zero exit cannot hide finalizer warnings."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest

RUNNER=Path(__file__).with_name('run-native.sh')

class NativeExitPolicy(unittest.TestCase):
    def test_every_declared_native_invocation_uses_diagnostic_guard(self):
        workflow=RUNNER.parents[2]/'.github/workflows/check.yml'
        commands=[line for line in workflow.read_text().splitlines() if 'nix develop --command desktop-test-python ' in line]
        self.assertGreater(len(commands),20)
        for command in commands:
            self.assertIn('bash tests/ci/run-native.sh ',command)
    def run_child(self,source):
        with tempfile.TemporaryDirectory() as root:
            result=subprocess.run(['bash',str(RUNNER),'synthetic',sys.executable,'-c',source],cwd=root,env=dict(os.environ,PYTHONWARNINGS='error'),capture_output=True,text=True)
            logs=list(Path(root).glob('artifacts/native-command-logs/*.log'))
            self.assertEqual(len(logs),1)
            return result,logs[0].read_text()
    def test_success_output_retained(self):
        result,log=self.run_child('print("Synthetic success")')
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn('Synthetic success',log)
    def test_failure_preserved(self):
        result,log=self.run_child('raise SystemExit(7)')
        self.assertEqual(result.returncode,7)
    def test_actual_zero_exit_finalizer_warning_fails(self):
        result,log=self.run_child('import tempfile; from pathlib import Path; root=tempfile.TemporaryDirectory(); p=Path(root.name)/"synthetic"; p.write_text("synthetic"); open(p).read(); root.cleanup()')
        self.assertEqual(result.returncode,1,result.stdout+result.stderr)
        self.assertIn('ResourceWarning:',log)
    def test_explicit_unraisable_diagnostic_fails(self):
        result,log=self.run_child('import sys; print("Exception ignored while finalizing synthetic fixture",file=sys.stderr)')
        self.assertEqual(result.returncode,1)

if __name__=='__main__':unittest.main()
