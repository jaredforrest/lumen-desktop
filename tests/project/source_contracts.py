"""Publication boundary: neutral names, no machine policy and valid source imports."""
import ast
import re
import sys
from pathlib import Path
root = Path(sys.argv[1]).resolve()
# Construct the rejected names to keep the source itself neutral.
forbidden = re.compile('|'.join(['a'+'pple',r'\b'+'m'+'ac'+r'\b','m'+'acos','m'+'acintosh']), re.I)
errors=[]
for p in root.rglob('*'):
    if '.git' in p.parts or '__pycache__' in p.parts:continue
    relative=str(p.relative_to(root))
    if forbidden.search(relative):errors.append('Branded path: '+relative)
    if not p.is_file():continue
    try:text=p.read_text()
    except UnicodeDecodeError:continue
    if forbidden.search(text):errors.append('Branded text: '+relative)
    if '/home/'+'jalio' in text:errors.append('Machine-specific path: '+relative)
    if p.suffix=='.py':
        try:ast.parse(text,filename=relative)
        except SyntaxError as e:errors.append(str(e))
    if p.suffix=='.nix':
        for path in re.findall(r'(?:import|callPackage)\s+(\.{1,2}/[\w./-]+)',text):
            if not (p.parent/path).exists():errors.append('Missing Nix import: '+relative+' -> '+path)
module = (root/'modules/desktop.nix').read_text()
for host_policy in ['boot.kernel', 'swapDevices', 'hardware.i2c', 'services.udev', 'environment.persistence', 'xdg.userDirs', 'kb_layout', 'max-items']:
    if host_policy in module: errors.append('Host policy in desktop module: '+host_policy)
if 'lumen-night.png' in module: errors.append('Removed wallpaper referenced by desktop module')
assert not errors,'\n'.join(errors)
print('PASS: neutral source, Python syntax, local Nix imports and publication boundary')
