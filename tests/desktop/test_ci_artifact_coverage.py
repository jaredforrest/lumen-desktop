"""Every declared native-test output must survive a failed workflow step."""
import fnmatch
from pathlib import Path
import re
import shlex
import unittest

class ArtifactCoverage(unittest.TestCase):
    def test_declared_outputs_are_retained_once(self):
        for path in sorted((Path(__file__).resolve().parents[2]/'.github/workflows').glob('*.yml')):
            with self.subTest(workflow=path.name):self.check_workflow(path.read_text())

    def check_workflow(self,workflow):
        retained=[line.strip() for line in workflow.splitlines() if line.startswith('            artifacts/')]
        self.assertEqual(len(retained),len(set(retained)), 'Duplicate artifact retention entry')
        outputs=[]
        for line in workflow.splitlines():
            if '--output ' not in line:continue
            words=shlex.split(line)
            for index,word in enumerate(words[:-1]):
                if word!='--output':continue
                output=words[index+1].removeprefix('$PWD/')
                if not output.startswith('artifacts/'):continue
                output=re.sub(r'\$\{[^}]+\}|\$[A-Za-z_][A-Za-z_0-9]*','fixture',output)
                outputs.append(output)
        self.assertTrue(outputs, 'No declared native test outputs found')
        for output in outputs:
            with self.subTest(output=output):
                self.assertTrue(any(fnmatch.fnmatchcase(output,pattern) for pattern in retained), 'Missing diagnostic retention for '+output)

if __name__=='__main__':unittest.main()
