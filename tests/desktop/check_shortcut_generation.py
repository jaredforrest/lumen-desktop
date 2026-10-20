"""Exercise actual module-derived command identities without dispatching input."""
import json,sys
from pathlib import Path
root=Path(sys.argv[1])
def bundle(name):
 value=json.loads((root/(name+'.json')).read_text());return value,json.loads(Path(value['registry']).read_text())
def action(document):return [row for row in document['bindings'] if row['action']=='window.fullscreen.toggle']
def hints(document):return [row for row in document['hints'] if row['action']=='window.fullscreen.toggle']
default,old=bundle('default');rebound,new=bundle('rebound')
assert old['valid'] and new['valid']
assert action(old)[0]['chord']=='Mod4+Control+f'
assert len(action(new))==1 and action(new)[0]['chord']=='Mod4+Control+x'
assert default['dispatch']['Mod4+Control+f']==rebound['dispatch']['Mod4+Control+x']
assert 'Mod4+Control+f' not in rebound['dispatch']
assert hints(new)[0]['bindings'][0]['chord']=='Mod4+Control+x'
for name in ('removed','replaced'):
 value=json.loads((root/(name+'.json')).read_text());assert value['valid'] and not action(value) and not hints(value)
print('Actual merged binding identities and generated hints agree after rebind, disable and replacement')
