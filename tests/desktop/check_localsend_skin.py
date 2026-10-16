"""Verify the pinned app changes only presentation files, not transfer behavior."""
import argparse,hashlib,importlib.util,json,shutil,tempfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',type=Path,required=True);p.add_argument('--font',type=Path,required=True);a=p.parse_args()
repo=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('skin',repo/'res/lumen-desktop/style_localsend.py');skin=importlib.util.module_from_spec(spec);spec.loader.exec_module(skin)
def hashes(root):return {str(f.relative_to(root)):hashlib.sha256(f.read_bytes()).hexdigest() for f in root.rglob('*') if f.is_file()}
with tempfile.TemporaryDirectory() as tmp:
 root=Path(tmp)/'app';shutil.copytree(a.source/'app',root)
 root.chmod(0o755)
 for item in root.rglob('*'):item.chmod(0o755 if item.is_dir() else 0o644)
 before=hashes(root);skin.apply(root,a.font);after=hashes(root)
 changed=sorted(name for name in before.keys()|after.keys() if before.get(name)!=after.get(name))
 assert changed==['assets/fonts/InterludeVariable.ttf','lib/config/theme.dart','lib/pages/home_page.dart','pubspec.yaml'],changed
 assert len([name for name in before if name.startswith('lib/provider/network/')])>10
 print(json.dumps(dict(passed=True,changed=changed,otherAppFilesUnchanged=len(before)-3)))
