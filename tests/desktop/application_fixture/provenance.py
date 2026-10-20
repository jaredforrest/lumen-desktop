"""Frozen source and immutable tool identity for disposable fixture runners."""
import hashlib,json,subprocess
from pathlib import Path


def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class FrozenSource:
    def __init__(self,source,script,dependencies):
        self.source=Path(source).resolve();self.script=Path(script).resolve()
        self.revision=self.git('rev-parse','HEAD')
        if self.git('status','--porcelain'):raise ValueError('Freeze source before native acceptance')
        self.source_hash=digest(self.script)
        self.dependencies={str(Path(path).resolve()):digest(Path(path).resolve()) for path in dependencies}
        if any(not path.startswith('/nix/store/') for path in self.dependencies):raise ValueError('Immutable fixture dependencies required')
    def git(self,*args):return subprocess.check_output(['git','-C',str(self.source),*args],text=True).strip()
    def finish(self,output,exit_code):
        unchanged=self.git('rev-parse','HEAD')==self.revision and not self.git('status','--porcelain') and digest(self.script)==self.source_hash
        tools_unchanged=all(digest(path)==expected for path,expected in self.dependencies.items())
        Path(output,'native-source-boundary.json').write_text(json.dumps(dict(sourceRevision=self.revision,sourceScript=str(self.script.relative_to(self.source)),sourceSha256=self.source_hash,dependencies=self.dependencies,sourceUnchanged=bool(unchanged),dependenciesUnchanged=tools_unchanged,exitCode=exit_code),indent=2)+'\n')
        if not unchanged or not tools_unchanged:raise ValueError('Fixture source or dependency changed during native acceptance')


def generated(root,output):
    root=Path(root)
    files={str(path.relative_to(root)):digest(path) for path in root.iterdir() if path.is_file() and path.suffix in ('.qml','.conf')}
    Path(output,'generated-harnesses.json').write_text(json.dumps(files,indent=2)+'\n')
