"""Native chooser results retain the document that requested them."""
import copy
from . import formats,live


def exercise(control,key,command,wait,provider,source,out,facts,checks,tree,walk):
 original_hash=formats.digest(source)
 backend=facts['privateChooser']['backendPid']
 def ui():return control.tool('ui_inspect')
 def choosers():return [node for node in walk(tree()) if node.get('pid')==backend and node.get('type') in ('con','floating_con')]
 def capture():return dict(content=live.snapshot(control,'writer'),info=control.command('writer','file.info'))
 def same(before):
  after=capture()
  # Selection is not content ownership; retain the complete document, path and dirty state.
  assert after['content']==before['content'],(before,after)
  assert all(after['info'].get(name)==before['info'].get(name) for name in ('path','dirty')),(before,after)
  assert formats.digest(source)==original_hash
  return after
 def reopen():
  control.tool('open_document',dict(path=str(source)))
  wait(lambda:control.command('writer','file.info').get('path')==str(source),'owned source reopened cleanly',provider)
  assert not control.command('writer','file.info')['dirty']
 def ask(opening=False):
  wait(lambda:any(node.get('id')==facts['native']['conId'] and node.get('pid')==facts['native']['pid'] and node.get('focused') for node in walk(tree())),'exact owned document regains native focus before next key',provider)
  key('-M','ctrl','-k','o','-m','ctrl') if opening else key('-k','F12')
  node=wait(lambda:next((row for row in choosers() if row.get('focused')),None),'actual pending native document chooser',provider)
  state=ui()
  if 'fileDialogOpen' in state:assert state['fileDialogOpen'] is True
  facts['chooserOwnerLastPending']=dict(window=node,ui=state)
  return node
 def choose(node,target,opening=False):
  key('-M','ctrl','-k','l','-m','ctrl');key('-M','ctrl','-k','a','-m','ctrl');key('--',str(target))
  if opening:key('-k','Tab')  # Commit GTK location completion before the one accept activation.
  key('-k','Return')
  wait(lambda:not any(row['id']==node['id'] for row in choosers()),'exact native chooser result returns',provider)
 def completed(target=None,opening=False):
  def terminal():
   value=ui()
   if 'fileDialogOpen' in value:
    assert type(value['fileDialogOpen']) is bool
    return value if value['fileDialogOpen'] is False else None
   # Preserve an actual failing baseline for the older provider without diagnostic fields.
   if target is not None and target.exists():return value
   if opening and value.get('dialog') is not None:return value
   return None
  value=wait(terminal,'chooser callback completion acknowledged by provider',provider)
  facts['chooserOwnerLastCompleted']=value
  return value

 replaced=ask();control.tool('new_document');control.command('writer','text.insert',dict(text='Replacement content must never be saved by an old picker'))
 expected=capture();target=out/'refused-replacement.docx';choose(replaced,target);state=completed(target)
 facts['chooserReplacement']=dict(before=expected,after=capture(),outputExists=target.exists(),ui=state)
 assert not target.exists(),'Late Save As wrote the replacement document'
 same(expected);assert 'document changed' in state.get('status','').lower();checks.append('Late physical Save As refuses a replacement document before output and preserves all unsaved content')

 reopen();edited=ask();control.command('writer','ui.backstage',dict(value=True,page='export'));assert ui()['ui']['backstage'] is True;control.command('writer','text.insert',dict(text='Revision while native chooser is open'))
 expected=capture();target=out/'refused-revision.docx';choose(edited,target);state=completed(target)
 assert not target.exists();same(expected);assert 'document changed' in state.get('status','').lower();assert state['ui']['backstage'] is False;command(['grim',str(out/'chooser-owner-refusal-visible.png')]);checks.append('Late physical Save As refuses newer edits and exposes its actionable refusal outside Backstage without clearing dirty state')

 reopen();canceled=ask();control.tool('new_document');control.command('writer','text.insert',dict(text='Cancellation preserves this newer document'))
 expected=capture();key('-k','Escape');wait(lambda:not any(row['id']==canceled['id'] for row in choosers()),'captured stale chooser Cancel',provider);completed();same(expected)
 checks.append('Canceling an old native chooser leaves replacement content and path unchanged')

 reopen();opening=ask(True);control.tool('new_document');control.command('writer','text.insert',dict(text='Open must not prompt to replace this bystander'))
 expected=capture();choose(opening,source,True);state=completed(opening=True);same(expected);assert state.get('dialog') is None;assert 'document changed' in state.get('status','').lower()
 checks.append('Late physical Open refuses its stale owner without opening a Save Changes prompt for a bystander')

 reopen();legitimate=ask();expected=capture();target=out/'legitimate-copy.docx';choose(legitimate,target);completed(target)
 wait(target.is_file,'legitimate same-owner Save As writes a copy',provider);assert formats.writer(target)==formats.writer(source);assert formats.digest(source)==original_hash
 info=control.command('writer','file.info');assert info.get('path')==str(target) and not info['dirty']
 after=live.snapshot(control,'writer');facts['chooserSavedLive']=after
 # An explicit Save legitimately updates only these documented core save stamps.
 comparable=[]
 for value in (expected['content'],after):
  value=copy.deepcopy(value)
  for field in ('created','modified','lastModifiedBy','revision'):value['document']['properties'].pop(field,None)
  comparable.append(value)
 assert comparable[0]==comparable[1]
 assert after['document']['properties']['revision']==expected['content']['document']['properties']['revision']+1
 facts['chooserLegitimateCopy']=dict(path=str(target),sha256=formats.digest(target),document=formats.writer(target));checks.append('Legitimate same-document native Save As persists independently parsed complete content and preserves the original')

 failed=ask();expected=capture();existing={row['id'] for row in choosers()}
 key('-M','ctrl','-k','l','-m','ctrl');key('-M','ctrl','-k','a','-m','ctrl');key('--','/source/nonexistent-chooser-output-folder/refused.docx');key('-k','Return')
 error=wait(lambda:next((row for row in choosers() if row['id'] not in existing),None),'actual native unavailable Save As destination error',provider)
 command(['grim',str(out/'chooser-owner-destination-error.png')]);key('-k','Escape');wait(lambda:not any(row['id']==error['id'] for row in choosers()),'owned destination error dismissed',provider)
 key('-k','Escape');wait(lambda:not any(row['id']==failed['id'] for row in choosers()),'unavailable chooser safely canceled',provider);completed();same(expected)
 checks.append('Unavailable native Save As destination remains an explicit chooser error and Cancel preserves content and saved path')

 canceled=ask();expected=capture();key('-k','Escape');wait(lambda:not any(row['id']==canceled['id'] for row in choosers()),'ordinary chooser Cancel',provider);completed();same(expected)
 checks.append('Ordinary native Save As Cancel remains usable and preserves current content and saved path')
 command(['grim',str(out/'chooser-owner-final.png')]);facts['chooserOwnershipScope']='Captured native document chooser owner/revision/path; remote control mutations are actual supported provider commands, not a mock document source'
