"""Actual origin prompt decisions, never debugger permission overrides."""
import json
from urllib.parse import urlsplit

def run(c):
 def snapshot(name):
  rows=c.accessibility.snapshot();(c.out/(name+'.json')).write_text(json.dumps(rows,indent=2));return rows
 def button(name,rows=None):
  candidates=[row for row in (rows if rows is not None else snapshot('permission-current')) if row.get('states',{}).get('showing') and row.get('pid')==c.main['pid'] and row.get('role')=='button' and row.get('name')==name]
  assert len(candidates)<=1,(name,candidates);return candidates[0] if candidates else None
 def activate(row):c.accessibility.focus(row);c.key('-k','space')
 def request(decision,expected):
  assert c.evaluate('Notification.permission')=='default';c.click_page('#permission')
  def control():
   rows=snapshot('notification-'+expected+'-controls');matches=[row for row in rows if row.get('states',{}).get('showing') and row.get('pid')==c.main['pid'] and row.get('role')=='button' and row.get('name')==decision]
   if not matches:return None
   assert any(row.get('role')=='alert' and row.get('name')==c.server.origin+' wants to: Show notifications' for row in rows),rows
   assert len(matches)==1;return matches[0]
  captured=c.wait(control,'actual origin notification '+decision+' control');c.capture('notification-'+expected+'-prompt.png');activate(captured)
  c.wait(lambda:c.evaluate("document.querySelector('#status').textContent")==expected and c.evaluate('Notification.permission')==expected,'actual origin notification '+expected);c.capture('notification-'+expected+'.png');return captured
 def reset():
  activate(c.wait(lambda:button('View site information'),'site controls after prompt closes'));rows=snapshot('site-permission-controls');assert any(row.get('role')=='alert' and row.get('name')==urlsplit(c.server.origin).netloc for row in rows),rows
  c.capture('site-permission-controls.png');activate(button('Reset permission',rows));c.wait(lambda:c.evaluate('Notification.permission')=='default','native origin permission reset');c.key('-k','Escape');c.key('-M','ctrl','-k','r','-m','ctrl');c.wait(lambda:c.evaluate("document.querySelector('#status')?.textContent")=='No permission requested' and c.evaluate('Notification.permission')=='default','actual page reload after permission reset')
 denied=request('Block','denied');c.checks.append('Real notification prompt denial updates requesting page and captured origin permission')
 reset();c.checks.append('Actual site controls reset denied notification permission to default without a debugger grant')
 granted=request('Allow','granted');c.checks.append('Real captured-origin notification Allow updates provider and requesting page')
 reset();c.checks.append('Actual site controls revoke granted notification permission back to default')
 (c.out/'notification-permission-result.json').write_text(json.dumps(dict(origin=c.server.origin,denied=denied,granted=granted,finalPermission=c.evaluate('Notification.permission')),indent=2))
