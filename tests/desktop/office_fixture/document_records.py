"""Capability records that preserve the distinction between payload and complete workflow."""
import hashlib,json,re
from pathlib import Path
from .records import PROVIDERS

def generate(path,native_paths=()):
 raw=Path(path).read_bytes();value=json.loads(raw)
 if value.get('passed') is not True or len(value.get('checks',[]))!=5:raise ValueError('Five successful scoped document observations required')
 reference=dict(path=str(path),sha256=hashlib.sha256(raw).hexdigest(),source=value['source'])
 native=[]
 for native_path in native_paths:
  payload=Path(native_path).read_bytes();result=json.loads(payload)
  app=result.get('app');provider=PROVIDERS.get(app)
  if result.get('passed') is not True or provider is None or not result.get('facts',{}).get('native',{}).get('executable','').endswith('/.'+provider[1]+'-native'):raise ValueError('Successful exact native office provider evidence required')
  native.append((result,dict(path=str(native_path),sha256=hashlib.sha256(payload).hexdigest(),source=result['source'])))
 records=[]
 for app,(provider,name,_) in PROVIDERS.items():
  tools=[tool for tool in value['installedTools'] if tool.endswith('/'+name+'-cli')]
  if len(tools)!=1:raise ValueError('One exact installed provider identity required')
  version=re.search(r'-'+re.escape(name)+r'-([^/]+)/bin/',tools[0])
  if not version:raise ValueError('Exact provider version required')
  for capability in ('T26-DOC-008','T26-DOC-010'):
   selected=value['checks'][:3] if app=='writer' and capability=='T26-DOC-008' else value['checks'][3:] if app=='sheet' and capability=='T26-DOC-010' else []
   supported=selected[:1]
   differences=['Saved-file reopen has no retained session snapshots; this does not provide persistent document versions'] if app=='writer' and capability=='T26-DOC-008' else ['Command-line PDF payload is not a native print/options/destination workflow'] if selected else []
   records.append(dict(capability_id=capability,provider_app_id=provider,provider_version=version[1],supported_actions=supported,unsupported_actions=[dict(action='Persistent version history after saved-file reopen',reason='Observed session snapshot list is empty')] if app=='writer' and capability=='T26-DOC-008' else [],backend=dict(kind='installed_provider',tool=tools[0],sha256=value['installedTools'][tools[0]]),permission_set=dict(files='Disposable namespace only',network='Private loopback only',printers='No host printer service or device'),offline_policy=dict(tested='Observed actions ran without external networking',unassessed='External document and printer services'),scenario_results=[dict(action=check,evidence=reference) for check in selected],reference_evidence=[],compatibility_differences=differences,status='implemented_unverified' if selected else 'unassessed',unverified_actions=['Native version browsing/restoration/cancel and persistence'] if capability=='T26-DOC-008' else ['Native destination/options, cancellation and unavailable printer outcomes'],scope_complete=False))
 for record in records:
  if record['capability_id']!='T26-DOC-010':continue
  observed=[(result,reference) for result,reference in native if PROVIDERS[result['app']][0]==record['provider_app_id']]
  for result,reference in observed:
   actions=[check for check in result['checks'] if not check.startswith(('Installed profile','No-DRI'))]
   record['supported_actions'].extend(action for action in actions if action not in record['supported_actions'])
   record['scenario_results'].extend(dict(action=action,evidence=reference,backend='effects' if 'softwareEffectsProof' in result['facts'] else 'basic') for action in actions)
  if observed:
   delegates=[]
   for result,reference in observed:
    viewer=result['facts'].get('actualPdfViewer')
    if not viewer:continue
    executable=Path(viewer['executable'])
    if not str(executable).startswith('/nix/store/') or not executable.is_file():raise ValueError('Immutable delegated provider required')
    delegate=dict(executable=str(executable),sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),evidence=reference)
    if delegate not in delegates:delegates.append(delegate)
   record['backend']['delegates_to']=delegates
   record['status']='implemented_unverified';record['unverified_actions']=['Actual printer queue acceptance and unavailable-printer outcome; PDF file destination failure is tested separately']
   record['compatibility_differences']=['Print is delegated to the configured PDF viewer; print-to-file/cancel and unavailable file destination are tested without claiming physical printing'] if record['provider_app_id']==PROVIDERS['writer'][0] else ['Provider-owned Print to PDF supports a file destination; physical printer queues and submission are not implemented']
   if record['provider_app_id']!=PROVIDERS['writer'][0]:record['unsupported_actions'].append(dict(action='Physical printer queue selection and submission',reason='Provider-owned action implements PDF file output only'))
 return dict(schema=1,records=records,complete=False)
