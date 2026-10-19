"""Typed shortcut inventory and collision diagnostics, without input rewriting."""
import argparse
import json
import re
import shlex
import sys
from pathlib import Path

MODIFIERS = {'shift':'Shift','lock':'Lock','capslock':'Lock','control':'Control','ctrl':'Control','alt':'Mod1','mod1':'Mod1','mod2':'Mod2','mod3':'Mod3','mod4':'Mod4','super':'Mod4','mod5':'Mod5'}
MODIFIER_ORDER = ['Shift','Lock','Control','Mod1','Mod2','Mod3','Mod4','Mod5']
FLAGS = {'--release','--locked','--inhibited','--no-repeat','--whole-window','--border','--exclude-titlebar'}
FIELDS = {'id','action','chord','scope','appId','mode','device','trigger','owner','fallback','enabled'}

class RegistryError(ValueError):
    pass


def chord(value):
    if not isinstance(value,str) or not value or len(value)>512 or any(ord(c)<32 for c in value):
        raise RegistryError('Chord must be a non-empty bounded string')
    try:tokens=shlex.split(value)
    except ValueError as error:raise RegistryError('Chord quoting is invalid') from error
    flags=set();device='*'
    while tokens and tokens[0].startswith('--'):
        flag=tokens.pop(0)
        if flag.startswith('--input-device='):
            device=flag.split('=',1)[1]
            if not device:raise RegistryError('Input device must be explicit')
        elif flag in FLAGS:flags.add(flag)
        else:raise RegistryError('Unsupported binding option: '+flag)
    if len(tokens)!=1:raise RegistryError('A chord requires exactly one final key')
    parts=tokens[0].split('+');key=parts.pop();modifiers=[]
    if not key or not re.fullmatch(r'[A-Za-z0-9_.,/;\[\]\\`\-=]+',key):raise RegistryError('Use an explicit keysym name for the final key')
    for part in parts:
        modifier=MODIFIERS.get(part.lower())
        if modifier is None:raise RegistryError('Unknown physical modifier: '+part)
        if modifier in modifiers:raise RegistryError('A modifier occurs more than once')
        modifiers.append(modifier)
    if len(key)==1 and key.isalpha():key=key.lower()
    key={'esc':'escape','enter':'return'}.get(key.lower(),key.lower())
    return dict(key=key,modifiers=[m for m in MODIFIER_ORDER if m in modifiers],trigger='release' if '--release' in flags else 'press',device=device,locked='--locked' in flags,inhibited='--inhibited' in flags)


def binding(value):
    if not isinstance(value,dict) or set(value)-FIELDS:raise RegistryError('Binding has unknown fields or is not an object')
    for field in ('id','action','chord'):
        if not isinstance(value.get(field),str) or not value[field] or len(value[field])>512 or any(ord(c)<32 for c in value[field]):raise RegistryError('Binding requires bounded '+field)
    if 'enabled' in value and type(value['enabled']) is not bool:raise RegistryError('Enabled must be boolean')
    scope=value.get('scope','global')
    if scope not in ('global','application','current-application'):raise RegistryError('Unknown shortcut scope')
    app=value.get('appId','')
    if scope=='application' and (not isinstance(app,str) or not app or '*' in app):raise RegistryError('Application scope requires an exact appId')
    if scope!='application' and app:raise RegistryError('appId is only valid for application scope')
    mode=value.get('mode','default')
    if not isinstance(mode,str) or not mode or len(mode)>128:raise RegistryError('Mode must be a bounded string')
    parsed=chord(value['chord'])
    if 'trigger' in value:
        if value['trigger'] not in ('press','release'):raise RegistryError('Unknown trigger')
        if parsed['trigger']=='release' and value['trigger']!='release':raise RegistryError('Trigger conflicts with release flag')
        parsed['trigger']=value['trigger']
    if 'device' in value:
        device=value['device']
        if not isinstance(device,str) or not device or len(device)>256:raise RegistryError('Device must be a bounded identifier')
        if parsed['device']!='*' and parsed['device']!=device:raise RegistryError('Device conflicts with chord option')
        parsed['device']=device
    for field in ('owner','fallback'):
        if field in value and not isinstance(value[field],str):raise RegistryError(field+' must be a string')
    return {**value,'scope':scope,'appId':app,'mode':mode,'enabled':value.get('enabled',True),'physical':parsed}


def overlaps(left,right):
    a,b=left['physical'],right['physical']
    if left['mode']!=right['mode'] or a['key']!=b['key'] or a['modifiers']!=b['modifiers'] or a['trigger']!=b['trigger']:return False
    if a['device']!='*' and b['device']!='*' and a['device']!=b['device']:return False
    # --locked/--inhibited extend a binding's reach; they never make otherwise
    # identical bindings safely disjoint while the desktop is unlocked.
    if left['scope']==right['scope']=='application' and left['appId']!=right['appId']:return False
    return True


def validate(document):
    if not isinstance(document,dict) or set(document)-{'version','bindings'} or type(document.get('version')) is not int or document['version']!=1 or not isinstance(document.get('bindings'),list):raise RegistryError('Expected shortcut registry version 1 and a bindings list')
    if len(document['bindings'])>4096:raise RegistryError('Too many shortcut declarations')
    values=[];diagnostics=[];identities=set()
    for raw in document['bindings']:
        item=binding(raw)
        if item['id'] in identities:raise RegistryError('Duplicate binding ID: '+item['id'])
        identities.add(item['id']);values.append(item)
    for index,left in enumerate(values):
        if not left['enabled']:continue
        for right in values[index+1:]:
            if right['enabled'] and overlaps(left,right):diagnostics.append({'code':'shortcut_collision','bindings':[left['id'],right['id']],'actions':[left['action'],right['action']],'chord':'+'.join(left['physical']['modifiers']+[left['physical']['key']]),'trigger':left['physical']['trigger'],'message':'Overlapping active shortcut scopes; disable or explicitly rescope one declaration.'})
    return {'version':1,'valid':not diagnostics,'bindings':values,'diagnostics':diagnostics}


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('registry',type=Path);parser.add_argument('--output',type=Path);args=parser.parse_args()
    try:result=validate(json.loads(args.registry.read_text()))
    except (OSError,ValueError,TypeError) as error:
        print(json.dumps({'valid':False,'diagnostics':[{'code':'invalid_registry','message':str(error)}]}));return 2
    encoded=json.dumps(result,indent=2)+'\n'
    if args.output:args.output.write_text(encoded)
    else:print(encoded,end='')
    if not result['valid']:
        for diagnostic in result['diagnostics']:print('Shortcut collision: '+', '.join(diagnostic['bindings'])+' ('+diagnostic['chord']+')',file=sys.stderr)
        return 1
    return 0

if __name__=='__main__':raise SystemExit(main())
