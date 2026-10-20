"""Actual portal overwrite decisions against one disposable existing target."""
import hashlib,json

def run(c):
 target=c.downloads/'report.html';target.write_text('original disposable contents');original=hashlib.sha256(target.read_bytes()).hexdigest()
 def confirm(chooser,existing,name):
  def ready():
   rows=[row for row in c.windows() if row['id'] not in existing];assert len(rows)<=1,rows
   if not rows:return None
   row=rows[0];return row if row.get('focused') and row.get('visible') else None
  found=c.wait(ready,'real focused portal existing-target confirmation')
  assert found['pid']==c.portal_facts['backendPid'] and found.get('lumen_relationships',{}).get('parent')==chooser['id'],found
  assert found['id']!=chooser['id'];(c.out/('overwrite-'+name+'-tree.json')).write_text(json.dumps(c.tree(),indent=2));c.call('grim',c.out/('overwrite-'+name+'.png'));return found
 chooser=c.open_save();existing={row['id'] for row in c.windows()};c.choose_path(target,chooser,finish_entry=True);question=confirm(chooser,existing,'cancel')
 assert hashlib.sha256(target.read_bytes()).hexdigest()==original
 (c.out/'overwrite-confirmation-tree.json').write_text(json.dumps(c.tree(),indent=2));c.call('grim',c.out/'overwrite-confirmation.png')
 c.key('-k','Escape');c.wait(lambda:all(row['id']!=question['id'] for row in c.windows()),'actual portal overwrite Cancel')
 assert any(row['id']==chooser['id'] for row in c.windows());assert hashlib.sha256(target.read_bytes()).hexdigest()==original
 c.key('-k','Escape');c.wait(lambda:all(row['id']!=chooser['id'] for row in c.windows()),'chooser cancellation after overwrite refusal');assert c.wait(lambda:c.evaluate('document.hasFocus()'),'page focus after overwrite refusal')
 c.checks.append('Real portal overwrite Cancel preserves exact existing target hash and returns chooser/page focus')
 chooser=c.open_save();existing={row['id'] for row in c.windows()};c.choose_path(target,chooser,finish_entry=True);question=confirm(chooser,existing,'replace')
 assert hashlib.sha256(target.read_bytes()).hexdigest()==original
 c.key('-M','alt','-k','r','-m','alt');c.wait(lambda:all(row['id'] not in (question['id'],chooser['id']) for row in c.windows()),'explicit native portal Replace')
 c.wait(lambda:target.exists() and hashlib.sha256(target.read_bytes()).hexdigest()!=original,'captured target replaced after confirmation')
 data=target.read_bytes();assert b'Disposable local page' in data
 c.navigate('brave://downloads');row=c.wait(lambda:next((row for row in c.ui_downloads() if row['filePath']==str(target) and row['state']==2),None),'actual saved-page provider complete after Replace')
 item="Array.from(document.querySelector('downloads-manager').shadowRoot.querySelectorAll('downloads-item')).find(item=>item.data.id==="+json.dumps(row['id'])+")"
 c.capture_presented('('+item+").shadowRoot.querySelector('#content')",'overwrite-completed-download.png')
 (c.out/'overwrite-result.json').write_text(json.dumps(dict(target=str(target),beforeSha256=original,afterSha256=hashlib.sha256(data).hexdigest(),bytes=len(data),download=row,portal=c.portal_facts),indent=2))
 c.checks.append('Explicit native portal Replace commits exact captured target and completed provider record after confirmation')
