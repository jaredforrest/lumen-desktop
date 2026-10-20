"""Physical document Print surface observations for the retained installed providers."""
import json,time,os
from pathlib import Path
from . import formats

def inspect_print(app,control,key,command,wait,provider,source,out,facts,checks,pointer_events,tree,walk,accessibility):
 before=formats.digest(source)
 def ui():
  if app=='writer':return control.tool('ui_inspect')
  if app=='sheet':return json.loads(command([facts['documentProbeCli'],'send',str(facts['documentProbePort']),'ui.inspect','{}']))
  return control.tool('call_app',dict(method='ui.inspect'))
 facts['printBefore']=ui();key('-M','ctrl','-k','p','-m','ctrl')
 if app=='writer':wait(lambda:(value if (value:=ui()).get('ui',{}).get('backstagePage')=='print' and value.get('ui',{}).get('backstage') else None),'physical Print opens provider Print page',provider)
 else:time.sleep(.3)
 facts['printAfter']=ui();command(['grim',str(out/'print-page.png')]);assert formats.digest(source)==before
 if app=='writer':
  pointer_events(400,169,['p','r']);time.sleep(.5);facts['printHandoffAfter']=ui();facts['printTemporaryFiles']=[dict(path=str(path),sha256=formats.digest(path)) for path in sorted(Path('/tmp').glob('wordcraft-print-*.pdf'))];command(['grim',str(out/'print-handoff.png')])
  assert formats.digest(source)==before
  if not facts.get('pdfViewer'):
   wait(lambda:not ui().get('ui',{}).get('backstage'),'failed print handoff exposes the existing document status immediately',provider)
   assert 'print failed:' in (out/'wordcraft.log').read_text() and not facts['printTemporaryFiles'];checks.append('Unavailable viewer failure immediately reveals document status and removes its temporary PDF without source changes')
  if facts.get('pdfViewer'):
   viewer=wait(lambda:next((node for node in walk(tree()) if node.get('app_id') in ('brave-browser','brave') and node.get('pid')),None),'actual configured PDF browser maps after delayed handoff',provider)
   pdfs=list(Path('/tmp').glob('wordcraft-print-*.pdf'));assert len(pdfs)==1;pdf=pdfs[0];assert formats.package_kind(pdf)=='pdf'
   native_pid=viewer['pid'];cmdline=Path('/proc/'+str(native_pid)+'/cmdline').read_bytes().split(b'\0');assert any(str(pdf).encode() in arg for arg in cmdline), 'Actual viewer must own the captured print PDF argument'
   def painted_pdf():
    from PIL import Image
    command(['grim',str(out/'print-viewer.png')])
    with Image.open(out/'print-viewer.png') as image:
     rect=viewer['rect'];crop=image.convert('RGB').crop((rect['x']+50,rect['y']+180,rect['x']+rect['width']-50,rect['y']+rect['height']-30));colors=crop.getcolors(crop.width*crop.height) or [];white=sum(count for count,color in colors if min(color)>=245);return dict(whitePixels=white,area=crop.width*crop.height) if white>max(20000,crop.width*crop.height//8) else None
   facts['pdfViewerPaint']=wait(painted_pdf,'actual PDF viewer paints a document page beyond browser chrome',provider)
   facts['actualPdfViewer']=dict(native=viewer,executable=os.readlink('/proc/'+str(native_pid)+'/exe'),cmdline=[arg.decode() for arg in cmdline if arg],path=str(pdf),sha256=formats.digest(pdf));checks.append('Configured retained browser opens the exact temporary print PDF after delayed handoff without premature cleanup')
   key('-M','ctrl','-k','p','-m','ctrl')
   def print_rows():
    rows=accessibility.snapshot();facts['viewerPrintLastRows']=rows;owned=[row for row in rows if row.get('pid')==native_pid];return owned if any(row.get('name') in ('Save','Print') and row.get('role')=='button' for row in owned) else None
   facts['viewerPrintControls']=wait(print_rows,'actual PDF viewer Print preview exposes native controls',provider);command(['grim',str(out/'viewer-print-preview.png')]);key('-k','Escape')
   def print_absent():return not any(row.get('pid')==native_pid and row.get('name')=='Save' and row.get('role')=='button' and any(parent.get('name')=='Print' and parent.get('role')=='frame' for parent in row.get('ancestors',[])) for row in accessibility.snapshot())
   wait(print_absent,'physical Escape retires the captured Print preview',provider);checks.append('Retained PDF viewer opens a real Print preview with destination controls and physical Escape cancels')
   def print_chooser():
    key('-M','ctrl','-k','p','-m','ctrl');rows=wait(print_rows,'real Print preview ready before captured Save',provider);save=next(row for row in rows if row.get('name')=='Save' and row.get('role')=='button');bounds=accessibility.bounds(save);current=next(node for node in walk(tree()) if node.get('id')==viewer['id'] and node.get('pid')==native_pid);origin=(current['rect']['x']+current['window_rect']['x'],current['rect']['y']+current['window_rect']['y']);facts['printSaveTarget']=dict(control=save,bounds=bounds,origin=origin);pointer_events(origin[0]+bounds[0]+bounds[2]/2,origin[1]+bounds[1]+bounds[3]/2,['p','r'])
    return wait(lambda:next((node for node in walk(tree()) if node.get('pid') in (native_pid,facts['privateChooser']['backendPid']) and node.get('id')!=viewer['id'] and node.get('focused')),None),'exact owned Print Save chooser maps',provider)
   def chooser_gone(node):wait(lambda:not any(row.get('id')==node['id'] for row in walk(tree())),'captured Print Save chooser retires',provider)
   first=print_chooser();key('-k','Escape');chooser_gone(first);assert formats.digest(source)==before;checks.append('Actual Print Save chooser cancellation leaves the document and print PDF intact')
   second=print_chooser();ids={node['id'] for node in walk(tree()) if node.get('pid') in (native_pid,facts['privateChooser']['backendPid'])};key('-M','ctrl','-k','l','-m','ctrl');key('-M','ctrl','-k','a','-m','ctrl');key('--','/source/nonexistent-viewer-print-folder/failed.pdf');key('-k','Return');error=wait(lambda:next((node for node in walk(tree()) if node.get('pid') in (native_pid,facts['privateChooser']['backendPid']) and node['id'] not in ids and node.get('focused')),None),'Print Save unavailable destination error',provider);command(['grim',str(out/'viewer-print-unavailable.png')]);key('-k','Escape');chooser_gone(error);key('-k','Escape');chooser_gone(second);assert formats.digest(source)==before;checks.append('Real Print Save unavailable destination exposes an owned error and cancels without document mutation')
   third=print_chooser();printed=out/'viewer-printed.pdf';key('-M','ctrl','-k','l','-m','ctrl');key('-M','ctrl','-k','a','-m','ctrl');key('--',str(printed));key('-k','Return');chooser_gone(third);wait(printed.is_file,'actual Print Save PDF publication',provider);assert formats.package_kind(printed)=='pdf' and formats.digest(source)==before;tools=Path(facts['documentPdfTools']);text=command([str(tools/'bin/pdftotext'),str(printed),'-']);assert 'Workflow heading' in text and 'Second section.' in text;facts['nativePrintedPdf']=dict(path=str(printed),sha256=formats.digest(printed),text=text);checks.append('Actual retained viewer Print Save produces independently parsed PDF content with source intact')
   command(['swaymsg','[con_id='+str(viewer['id'])+'] kill']);wait(lambda:not any(node.get('id')==viewer['id'] for node in walk(tree())),'captured PDF viewer closes',provider)
 key('-k','Escape')
 if app=='writer':wait(lambda:not ui().get('ui',{}).get('backstage'),'physical Escape dismisses provider Print page',provider)
 facts['printAfterEscape']=ui();assert formats.digest(source)==before
 if app=='writer' and facts.get('privateChooser'):
  existing_pdfs={str(path):formats.digest(path) for path in out.glob('*.pdf')}
  assert facts.get('documentPdfTools'), 'Independent PDF inspection tools required'
  def choosers():return [node for node in walk(tree()) if node.get('pid')==facts['privateChooser']['backendPid'] and node.get('type') in ('con','floating_con')]
  def chooser():return wait(lambda:next((node for node in choosers() if node.get('focused')),None),'captured owned PDF chooser',provider)
  def absent(node):wait(lambda:not any(row['id']==node['id'] for row in choosers()),'captured PDF chooser retires',provider)
  def open_pdf():
   key('-M','ctrl','-k','p','-m','ctrl');wait(lambda:ui().get('ui',{}).get('backstagePage')=='print' and ui().get('ui',{}).get('backstage'),'Print page before PDF export',provider);pointer_events(400,272,['p','r']);return chooser()
  first=open_pdf();command(['grim',str(out/'print-pdf-cancel.png')]);key('-k','Escape');absent(first);assert formats.digest(source)==before and {str(path):formats.digest(path) for path in out.glob('*.pdf')}==existing_pdfs;checks.append('Physical PDF export chooser cancellation leaves source and output unchanged')
  second=open_pdf();ids={node['id'] for node in choosers()};key('-M','ctrl','-k','l','-m','ctrl');key('-M','ctrl','-k','a','-m','ctrl');key('--','/source/nonexistent-print-folder/failed.pdf');key('-k','Return')
  error=wait(lambda:next((node for node in choosers() if node['id'] not in ids),None),'owned PDF chooser unavailable location error',provider);command(['grim',str(out/'print-pdf-unavailable.png')]);key('-k','Escape');absent(error);key('-k','Escape');absent(second);assert formats.digest(source)==before and {str(path):formats.digest(path) for path in out.glob('*.pdf')}==existing_pdfs;checks.append('Unavailable PDF destination is an owned chooser error and cancellation preserves source')
  third=open_pdf();target=out/'print-copy.pdf';key('-M','ctrl','-k','l','-m','ctrl');key('-M','ctrl','-k','a','-m','ctrl');key('--',str(target));key('-k','Return');absent(third);wait(target.is_file,'explicit PDF copy publication',provider);assert formats.package_kind(target)=='pdf' and formats.digest(source)==before
  tools=Path(facts['documentPdfTools']);text=command([str(tools/'bin/pdftotext'),str(target),'-']);metadata=command([str(tools/'bin/pdfinfo'),str(target)]);assert 'Workflow heading' in text and 'Second section.' in text and 'Pages:' in metadata;facts['nativePdfCopy']=dict(path=str(target),sha256=formats.digest(target),text=text,metadata=metadata);checks.append('Actual Print-page PDF chooser save produces independently parsed document content without source mutation')
  key('-k','Escape')

 checks.append('Physical Print invocation and Escape leave the captured source unchanged; exact provider UI observations retained')
 facts['documentProbeScope']='Print surface observation only; no printer acceptance or complete destination workflow claim'
