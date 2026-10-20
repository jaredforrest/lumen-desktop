"""Independent real download UI actions and exact disposable file outcomes."""
import hashlib,json
from server import DIGEST

def run(context):
 c=context
 def begin(name):
  existing={row['id'] for row in c.windows()};c.click_page('#'+name);chooser=c.wait(lambda:next((row for row in c.windows() if row['id'] not in existing),None),'actual independent download target chooser');assert chooser.get('lumen_relationships',{}).get('parent')==c.main['id'];c.capture('download-'+name+'-chooser.png');c.choose_path(c.downloads/('fixture-'+name+'.bin'),chooser);c.wait(lambda:all(row['id']!=chooser['id'] for row in c.windows()),'download target chooser acceptance')
 def control(identity,selector):
  item="Array.from(document.querySelector('downloads-manager').shadowRoot.querySelectorAll('downloads-item')).find(item=>item.data.id==="+json.dumps(identity)+")"
  return '('+item+').shadowRoot.querySelector('+json.dumps(selector)+')'
 begin('fixed');c.navigate('brave://downloads');c.wait(lambda:c.evaluate("!!document.querySelector('downloads-manager')"),'actual downloads manager')
 def captured(name,state):return next((row for row in c.ui_downloads() if row['fileName']=='fixture-'+name+'.bin' and row['state']==state),None)
 running=c.wait(lambda:captured('fixed',0),'independent actual running download');target=c.downloads/'fixture-fixed.bin'
 assert not target.exists() and list(c.downloads.glob('*.crdownload')) and not c.server.snapshot()[0]['complete']
 c.capture_presented(control(running['id'],'#content'),'download-running.png');c.checks.append('Independent gated download is genuinely running with partial file and no completed target')
 def action(identity,selector):c.click_expression(control(identity,'#more-actions'));c.click_expression(control(identity,selector))
 action(running['id'],'#pause-or-resume');c.wait(lambda:captured('fixed',3),'actual paused download');assert not target.exists();c.capture_presented(control(running['id'],'#content'),'download-paused.png')
 action(running['id'],'#pause-or-resume');c.wait(lambda:captured('fixed',0),'actual resumed download');c.checks.append('Captured native download overflow actions pause and resume the same provider identity')
 c.server.gates['fixed'].set();complete=c.wait(lambda:captured('fixed',2),'actual completed independent download');c.wait(lambda:target.exists() and hashlib.sha256(target.read_bytes()).hexdigest()==DIGEST,'complete download exact bytes');assert not list(c.downloads.glob('*.crdownload'));c.capture_presented(control(complete['id'],'#content'),'download-complete.png');c.checks.append('Independent download completion requires provider complete plus exact whole-file hash and no partial')
 c.navigate(c.server.origin+'/page');c.wait(lambda:c.evaluate('document.title')=='Fixture browser workflow','return to local page');begin('cancel');c.navigate('brave://downloads');c.wait(lambda:captured('cancel',0),'second independent running download');cancel=captured('cancel',0);action(cancel['id'],'#cancel');cancelled=c.wait(lambda:captured('cancel',1),'actual cancelled provider row');c.server.gates['cancel'].set();c.wait(lambda:not list(c.downloads.glob('*.crdownload')),'cancelled partial cleanup');assert not (c.downloads/'fixture-cancel.bin').exists();assert hashlib.sha256(target.read_bytes()).hexdigest()==DIGEST;c.capture_presented(control(cancelled['id'],'#content'),'download-cancelled.png');c.checks.append('Captured Cancel affects only second transfer and removes its partial without changing completed neighbor')
 (c.out/'download-outcomes.json').write_text(json.dumps(dict(running=running,complete=complete,cancelled=cancelled,server=c.server.snapshot(),completeSha256=DIGEST),indent=2))
