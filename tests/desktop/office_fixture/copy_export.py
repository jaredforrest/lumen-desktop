"""Physical copy export and captured conversion decisions preserve user content."""
from xml.etree import ElementTree
from zipfile import ZipFile
from . import formats,live

def exercise(control,key,pointer_events,command,wait,provider,source,out,facts,checks,tree,walk,accessibility,ext,release_pointer):
 owner=facts['native'];backend=facts['privateChooser']['backendPid'];original=formats.digest(source)
 def ui():return control.tool('ui_inspect')
 def snapshot():return dict(content=live.snapshot(control,'writer'),info=control.command('writer','file.info'))
 def choosers():return [row for row in walk(tree()) if row.get('pid')==backend and row.get('type') in ('con','floating_con')]
 def button(name):
  rows=accessibility.snapshot();facts['copyExportLastControls']=rows
  return next((row for row in rows if row.get('pid')==owner['pid'] and row.get('name')==name and row.get('role') in ('button','push button') and row.get('states',{}).get('showing')),None)
 def click(row,events=('p','r')):
  bounds=accessibility.bounds(row);native=next(row for row in walk(tree()) if row.get('id')==owner['conId'] and row.get('pid')==owner['pid']);origin=(native['rect']['x']+native['window_rect']['x'],native['rect']['y']+native['window_rect']['y']);facts['copyExportCapturedControl']=dict(button=row,bounds=bounds,origin=origin);pointer_events(origin[0]+bounds[0]+bounds[2]/2,origin[1]+bounds[1]+bounds[3]/2,list(events))
 def owned_button(name):return wait(lambda:button(name),'actual owned '+name+' control',provider)
 def export(format):
  control.command('writer','ui.backstage',dict(value=True,page='export'))
  click(owned_button({'docx':'Word document (*.docx)','odt':'OpenDocument Text (*.odt)'}[format]))
 def chooser():return wait(lambda:next((row for row in choosers() if row.get('focused')),None),'owned native export chooser',provider)
 def choose(node,target):
  key('-M','ctrl','-k','l','-m','ctrl');key('-M','ctrl','-k','a','-m','ctrl');key('--',str(target));key('-k','Return')
  wait(lambda:not any(row['id']==node['id'] for row in choosers()) and ui().get('fileDialogOpen') is False,'captured chooser completion',provider)
 def warning():
  state=wait(lambda:(value if (value:=ui()).get('dialog') or value.get('fileDialogOpen') else None),'conversion warning or unexpected chooser',provider)
  assert not state.get('fileDialogOpen') and not choosers(),'Lossy format opened a destination chooser without explicit consent'
  assert state.get('dialog',{}).get('dialog')=='formatLoss',state
  assert state['dialog'].get('loses_password') is True,state
  rows=accessibility.snapshot();assert any(row.get('pid')==owner['pid'] and 'converted file will be unencrypted' in row.get('name','') for row in rows),rows
  return state
 def same(expected):assert snapshot()==expected and formats.digest(source)==original
 control.command('writer','caret.docEnd',{});control.command('writer','text.insert',dict(text='Unsaved copy export ownership marker'))
 if ext=='odt':control.command('writer','file.encrypt',dict(password='Synthetic conversion protection'))
 before=snapshot();assert before['info']['dirty'] and before['info'].get('path')==str(source)
 export(ext)
 if ext=='docx':
  node=chooser();target=out/'explicit-document-copy.docx';choose(node,target);wait(target.is_file,'document copy publication',provider);same(before)
  parsed=formats.writer(target);assert any('Unsaved copy export ownership marker' in row['text'] for row in parsed['paragraphs']);facts['copyExportOutcome']=dict(before=before,after=snapshot(),output=parsed,outputSha256=formats.digest(target))
  checks.append('Physical native document-copy export preserves full active content, path and dirty state')
  export('docx');node=chooser();mismatch=out/'mismatched-copy.odt';choose(node,mismatch)
  wait(lambda:'selected filename must end' in str(ui().get('status','')),'explicit mismatched export refusal',provider);assert not mismatch.exists();same(before)
  checks.append('Renaming a DOCX export to ODT refuses before conversion or publication and preserves the working document')
  return
 warning();command(['grim',str(out/'format-loss-before-consent.png')]);click(owned_button('Cancel'));wait(lambda:ui().get('dialog') is None,'conversion Cancel',provider);same(before);assert not choosers()
 checks.append('Physical conversion Cancel creates no chooser or output and preserves the complete unsaved document')
 export('odt');warning();held=owned_button('Continue');click(held,('p',));control.command('writer','text.insert',dict(text=' Revision while conversion is held'))
 expected=snapshot();wait(lambda:ui().get('dialog') is None,'changed owner retires conversion decision',provider);release_pointer();same(expected);assert not choosers()
 checks.append('Held Continue cannot dispatch after the captured document revision changes')
 export('odt');warning();click(owned_button('Continue'));node=chooser();target=out/'explicit-converted-copy.odt';choose(node,target);wait(target.is_file,'consented ODT copy publication',provider);same(expected)
 assert formats.package_kind(target)=='odt'
 with ZipFile(target) as archive:text=''.join(ElementTree.fromstring(archive.read('content.xml')).itertext())
 assert 'Unsaved copy export ownership marker' in text and 'Revision while conversion is held' in text;facts['convertedCopy']=dict(sha256=formats.digest(target),text=text)
 checks.append('Physical conversion Continue and native destination publish parsed ODT content while retaining the dirty working document')
 key('-k','F12')
 node=chooser();save_target=out/'converted-save-as.odt';choose(node,save_target);warning();click(owned_button('Cancel'));wait(lambda:ui().get('dialog') is None,'post-picker conversion Cancel',provider);same(expected);assert not save_target.exists()
 checks.append('Actual Save As selected lossy format requires a second captured decision and Cancel leaves no output')
 key('-k','F12');node=chooser();choose(node,save_target);warning();save_target.write_bytes(b'Newer independently created output');click(owned_button('Continue'))
 wait(lambda:ui().get('dialog') is None,'changed destination refusal retires warning',provider);same(expected);assert save_target.read_bytes()==b'Newer independently created output'
 assert 'confirmed output' in str(ui().get('status','')).lower();checks.append('A destination created during confirmation is preserved and cannot be silently overwritten')
 save_target.unlink();key('-k','F12');node=chooser();choose(node,save_target);warning();click(owned_button('Continue'))
 wait(lambda:control.command('writer','file.info').get('path')==str(save_target),'confirmed Save As becomes active path',provider);assert formats.digest(source)==original and save_target.is_file();assert not control.command('writer','file.info')['dirty'];assert formats.package_kind(save_target)=='odt'
 checks.append('Explicit valid captured Save As continuation publishes the conversion and follows normal saved-path policy')
