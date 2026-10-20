"""Provider-scoped compatibility records without promoting partial action evidence."""
import hashlib,json,re
from pathlib import Path
PROVIDERS={'writer':('ai.storyteller.wordcraft','wordcraft','docx'),'sheet':('ai.storyteller.gridcraft','gridcraft','xlsx'),'slides':('ai.storyteller.deckcraft','deckcraft','pptx')}
SCOPES={'APP-OFFICE-01':['interactive styling, paragraphs, sections, tables, images, links, stories and review operations'], 'APP-OFFICE-02':['interactive calculation, formatting, sorting, filtering, charts, sheets and print ranges'], 'APP-OFFICE-03':['interactive layouts, notes, transitions and presenter view'], 'APP-OFFICE-04':['template creation, recent-entry removal, killed-process recovery and newer-source protection'], 'APP-OFFICE-05':['conversion-loss warning cancellation, save failure and print-dialog acceptance'], 'APP-OFFICE-06':['collaboration, change review and version history assessed separately'], 'APP-OFFICE-07':['interactive PDF navigation, search, selection, zoom, rotation, annotations, forms and signatures'], 'WF-OFFICE-01':['complete native reopen, reversible edit, dirty-close Save/Discard/error and killed-process recovery']}

def load(path):
 raw=Path(path).read_bytes();return json.loads(raw),dict(path=str(path),sha256=hashlib.sha256(raw).hexdigest())

def generate(cli_path,gui_paths=()):
 cli,cli_ref=load(cli_path)
 if cli.get('harnessPassed') is not True:raise ValueError('A successful scoped CLI report is required')
 gui=[]
 for path in gui_paths:
  value,ref=load(path)
  if value.get('passed') is not True:raise ValueError('GUI report did not pass')
  exe=value.get('facts',{}).get('native',{}).get('executable','')
  matches=[name for name,(_,provider,_) in PROVIDERS.items() if '/.'+provider+'-native' in exe]
  if len(matches)!=1:raise ValueError('GUI provider identity is not exact')
  gui.append((matches[0],value,ref))
 records=[]
 for app,(provider,name,ext) in PROVIDERS.items():
  tools=[path for path in cli.get('installedTools',{}) if path.endswith('/'+name+'-cli')]
  if len(tools)!=1:raise ValueError('Expected one exact provider tool')
  match=re.search(r'-'+re.escape(name)+r'-([^/]+)/bin/',tools[0])
  if not match:raise ValueError('Provider version was not captured')
  for capability in ['APP-OFFICE-0'+str({'writer':1,'sheet':2,'slides':3}[app]),'APP-OFFICE-04','APP-OFFICE-05','APP-OFFICE-06','WF-OFFICE-01']:
   observations=[]
   for observed in cli.get('capabilities',[]):
    if observed.get('id')!=capability:continue
    action=observed.get('action','')
    if capability=='APP-OFFICE-05' and not action.startswith(app+' '):continue
    if capability=='WF-OFFICE-01' and not action.startswith(ext+' '):continue
    observations.append(dict(kind='cli_action',evidence=cli_ref,result=observed))
   for gui_app,value,ref in gui:
    if gui_app==app and capability=='WF-OFFICE-01':
     observations.extend(dict(kind='native_action',evidence=ref,result=dict(action=check,status='fixture_verified')) for check in value.get('checks',[]))
   supported=[x['result']['action'] for x in observations if x['result'].get('status')=='fixture_verified']
   unsupported=[dict(action=x['result']['action'],outcome=x['result']['status'],reason=x['result'].get('reason')) for x in observations if x['result'].get('status') in ('unsupported','provider_defect')]
   differences=[x['result'] for x in observations if x['result'].get('status') in ('degraded','provider_defect')]
   records.append(dict(capability_id=capability,provider_app_id=provider,provider_version=match[1],supported_actions=supported,unsupported_actions=unsupported,backend=dict(kind='installed_provider',tool=tools[0],sha256=cli['installedTools'][tools[0]],scope='Synthetic CLI and explicitly listed native scenarios only'),permission_set=dict(files='Disposable namespace artifact files only',network='Loopback only; no external routes',devices='No host display, audio, camera or render devices'),offline_policy=dict(tested='All retained scenarios ran without external networking',unassessed='Provider network features and accounts'),scenario_results=observations,reference_evidence=[],compatibility_differences=differences,status='implemented_unverified' if supported or unsupported or differences else 'unassessed',unverified_actions=SCOPES[capability],scope_complete=False))
 records.append(dict(capability_id='APP-OFFICE-07',provider_app_id=None,provider_version=None,supported_actions=[],unsupported_actions=[],backend=None,permission_set={},offline_policy={'unassessed':'No interactive PDF provider has been selected or assessed'},scenario_results=[],reference_evidence=[],compatibility_differences=[],status='unassessed',unverified_actions=SCOPES['APP-OFFICE-07'],scope_complete=False))
 return dict(schema=1,records=records,complete=False,notes=['Historical CLI capability entries are scoped action observations, not full capability records.','No interactive reference platform measurements are available.','An untested action is not an unsupported or externally blocked action.'])
