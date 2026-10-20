"""Private compatibility-server account and CLI workflow; never a live vault."""
import argparse,base64,json,os,secrets,ssl,subprocess as sp,sys,time,urllib.request,urllib.parse,urllib.error
from pathlib import Path
from application_fixture.isolation import run
from application_fixture.provenance import FrozenSource
from application_fixture.offscreen import immutable_file
from browser_fixture.protocol import PipeTransport
from private_native_bus import start_private_bus
from vault_artifacts import ArtifactSecrets,run_cli,cleanup
from vault_https import PrivateHttps

p=argparse.ArgumentParser()
for name in ('server','web','cli','browser','bus','tls_tool','trust_tool','font_config','output'):p.add_argument('--'+name.replace('_','-'),type=Path,required=True)
p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('server','cli','browser','bus','tls_tool','trust_tool','font_config'):setattr(a,name,immutable_file(getattr(a,name)))
a.web=a.web.resolve();assert str(a.web).startswith('/nix/store/') and a.web.is_dir()
if not a.inside:
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 frozen=FrozenSource(repo,__file__,[a.server,a.cli,a.browser,a.bus,a.tls_tool,a.trust_tool,a.font_config,immutable_file(sys.executable)])
 command=[str(immutable_file(sys.executable)),'/source/tests/desktop/disposable_vault.py','--inside','--output','/artifacts']+sum((['--'+name.replace('_','-'),str(getattr(a,name))]for name in ('server','web','cli','browser','bus','tls_tool','trust_tool','font_config')),[])
 code=1
 try:code=run(repo,a.output,command)
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
assert repo==Path('/source') and Path.home()==Path('/home/fixture') and not Path('/.old-root').exists()
os.umask(0o077)
out=a.output;origin='https://127.0.0.1:8237';env=dict(os.environ,FONTCONFIG_FILE=str(a.font_config),PATH=str(a.bus.parent)+':'+os.environ.get('PATH',''));data=Path.home()/'vault';data.mkdir(mode=0o700)
server_env=dict(env,DATA_FOLDER=str(data),WEB_VAULT_FOLDER=str(a.web),ROCKET_ADDRESS='127.0.0.1',ROCKET_PORT='8238',DOMAIN=origin,SIGNUPS_ALLOWED='true',SIGNUPS_VERIFY='false',LOG_LEVEL='error',IP_HEADER='none')
certificate=Path.home()/'fixture-cert.pem';key=Path.home()/'fixture-key.pem'
sp.run([str(a.tls_tool),'req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=Disposable fixture','-addext','subjectAltName=IP:127.0.0.1,DNS:localhost','-keyout',str(key),'-out',str(certificate)],check=True,stdout=sp.DEVNULL,stderr=sp.PIPE,timeout=10)
trust=Path.home()/'.pki/nssdb';trust.mkdir(mode=0o700,parents=True)
sp.run([str(a.trust_tool),'-N','-d','sql:'+str(trust),'--empty-password'],check=True,capture_output=True,timeout=5)
sp.run([str(a.trust_tool),'-A','-d','sql:'+str(trust),'-n','Disposable fixture','-t','C,,','-i',str(certificate)],check=True,capture_output=True,timeout=5)
context=ssl.create_default_context(cafile=str(certificate));env['NODE_EXTRA_CA_CERTS']=str(certificate)
https=None
guard=ArtifactSecrets();email=guard.add('fixture-'+secrets.token_hex(6)+'@example.invalid');password=guard.add(secrets.token_urlsafe(32));process=None;providers=[]
def wait(predicate,label,seconds=15):
 deadline=time.monotonic()+seconds
 while True:
  value=predicate()
  if value:return value
  if time.monotonic()>=deadline:raise TimeoutError(label)
  time.sleep(.05)
def reachable():
 try:
  with urllib.request.urlopen(origin+'/alive',timeout=1,context=context)as response:return response.status==200
 except urllib.error.HTTPError as error:error.close();return False
 except OSError:return False
def cli(*args,session=None,should_succeed=True):
 local=dict(env,BITWARDENCLI_APPDATA_DIR=str(Path.home()/'cli'),BW_NOINTERACTION='true',LUMEN_FIXTURE_PASSWORD=password)
 if session:local['BW_SESSION']=session
 result=run_cli([str(a.cli),*args],local)
 if (result.returncode==0)!=should_succeed:raise RuntimeError('Unexpected disposable CLI outcome: '+args[0])
 return result.stdout.strip()
try:
 start_private_bus(env,out,providers);server_env['DBUS_SESSION_BUS_ADDRESS']=env['DBUS_SESSION_BUS_ADDRESS']
 with (out/'server.log').open('w')as log:process=sp.Popen([str(a.server)],env=server_env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 https=PrivateHttps(certificate,key,8238)
 wait(reachable,'Private compatibility server readiness')
 profile=Path.home()/'browser';profile.mkdir(mode=0o700)
 command=[str(a.browser),'--user-data-dir='+str(profile),'--headless','--disable-gpu','--no-first-run','--no-default-browser-check','--remote-debugging-pipe','about:blank']
 with (out/'browser.log').open('w')as log,PipeTransport(command,env,log)as browser:
  browser.request('Browser.getVersion',timeout=15)
  target=next(row for row in browser.request('Target.getTargets')['targetInfos']if row['type']=='page')
  session=browser.request('Target.attachToTarget',dict(targetId=target['targetId'],flatten=True))['sessionId']
  def request(method,params=None):return browser.request(method,params,session=session)
  def evaluate(expression):
   value=request('Runtime.evaluate',dict(expression=expression,returnByValue=True))
   if value.get('exceptionDetails'):raise RuntimeError('Disposable web form script rejected')
   return value['result'].get('value')
  def diagnostic():
    observation=evaluate('({text:document.body.innerText.slice(0,8000),fields:Array.from(document.querySelectorAll("input")).map(e=>({id:e.id,type:e.type,name:e.name,valid:e.validity.valid,length:e.value.length,classes:e.className,checked:e.type==="checkbox"?e.checked:undefined})),buttons:Array.from(document.querySelectorAll("button")).map(e=>({label:e.textContent.trim(),disabled:e.disabled}))})')
    (out/'registration-diagnostic.json').write_text(guard.redact(json.dumps(observation)))
    responses=[dict(path=urllib.parse.urlsplit(e['params']['response']['url']).path,status=e['params']['response']['status'])for e in browser.events if e.get('method')=='Network.responseReceived']
    responses.extend(dict(failed=e['params'].get('errorText'),blocked=e['params'].get('blockedReason'))for e in browser.events if e.get('method')=='Network.loadingFailed')
    (out/'registration-network.json').write_text(json.dumps(responses,indent=2))
    errors=[e['params']['exceptionDetails'].get('text')for e in browser.events if e.get('method')=='Runtime.exceptionThrown']
    errors.extend([arg.get('value',arg.get('description')) for arg in e['params'].get('args',[])]for e in browser.events if e.get('method')=='Runtime.consoleAPICalled'and e['params'].get('type')in('error','warning'))
    (out/'registration-errors.json').write_text(guard.redact(json.dumps(errors)))
  def fill(selector,value):
   try:wait(lambda:evaluate('!!document.querySelector('+json.dumps(selector)+')'),'Expected registration field')
   except TimeoutError:
    diagnostic()
    raise
   evaluate('document.querySelector('+json.dumps(selector)+').focus()')
   for character in value:request('Input.dispatchKeyEvent',dict(type='char',text=character,unmodifiedText=character))
   request('Input.dispatchKeyEvent',dict(type='keyDown',key='Tab',code='Tab',windowsVirtualKeyCode=9))
   request('Input.dispatchKeyEvent',dict(type='keyUp',key='Tab',code='Tab',windowsVirtualKeyCode=9))
  def button(label):
   expression='Array.from(document.querySelectorAll("button,a")).find(e=>e.textContent.trim()==='+json.dumps(label)+')'
   wait(lambda:evaluate('!!('+expression+')'),'Expected registration action')
   evaluate('(()=>{const b=('+expression+');if(b.disabled||!b.form)throw Error("Registration form unavailable");b.form.requestSubmit(b);return true})()')
  payload=json.dumps(dict(email=email,name='Disposable fixture',receiveMarketingEmails=False)).encode()
  provision=urllib.request.Request(origin+'/identity/accounts/register/send-verification-email',data=payload,headers={'Content-Type':'application/json'})
  with urllib.request.urlopen(provision,timeout=5,context=context)as response:registration_token=response.read().decode()
  if registration_token.startswith(chr(34)):registration_token=json.loads(registration_token)
  assert isinstance(registration_token,str) and registration_token
  guard.add(registration_token)
  with urllib.request.urlopen(origin+'/api/config',timeout=5,context=context)as response:assert response.status==200 and isinstance(json.loads(response.read()),dict)
  request('Page.enable');request('Network.enable');request('Runtime.enable');request('Page.navigate',dict(url=origin+'/#/finish-signup?'+urllib.parse.urlencode(dict(email=email,token=registration_token))))
  fill('input#input-password-form_new-password',password);fill('input#input-password-form_new-password-confirm',password)
  evaluate('(()=>{const e=document.querySelector("input[formcontrolname=checkForBreaches]");if(e&&e.checked)e.click();return true})()')
  button('Create account')
  try:wait(lambda:evaluate('!document.querySelector("input#input-password-form_new-password")'),'Registration completion')
  except TimeoutError:diagnostic();raise
 cli('config','server',origin)
 token=cli('login',email,'--passwordenv','LUMEN_FIXTURE_PASSWORD','--raw');assert token and '\n'not in token;guard.add(token)
 status=json.loads(cli('status',session=token));assert status['status']=='unlocked' and status['serverUrl']==origin
 item=json.loads(cli('get','template','item',session=token));item_password=guard.add(secrets.token_urlsafe(32))
 item.update(name='Disposable fixture item',type=1,login=dict(username='disposable-user',password=item_password,totp=None,uris=[dict(uri='https://example.invalid',match=None)]))
 def encode(value):return guard.add(base64.b64encode(json.dumps(value).encode()).decode())
 created=json.loads(cli('create','item',encode(item),session=token));item_id=created['id'];assert created['login']['password']==item_password
 fetched=json.loads(cli('get','item',item_id,session=token));assert fetched['login']['password']==item_password
 replacement=guard.add(secrets.token_urlsafe(32));fetched['login']['password']=replacement
 edited=json.loads(cli('edit','item',item_id,encode(fetched),session=token));assert edited['login']['password']==replacement
 cli('sync',session=token);cli('lock',session=token);assert json.loads(cli('status'))['status']=='locked'
 cli('get','item',item_id,should_succeed=False)
 token=guard.add(cli('unlock','--passwordenv','LUMEN_FIXTURE_PASSWORD','--raw'));assert json.loads(cli('status',session=token))['status']=='unlocked'
 assert json.loads(cli('get','item',item_id,session=token))['login']['password']==replacement
 cli('delete','item',item_id,session=token);cli('sync',session=token)
 assert item_id not in [row['id']for row in json.loads(cli('list','items',session=token))]
 cli('logout',session=token);assert json.loads(cli('status'))['status']=='unauthenticated'
 result=dict(passed=True,checks=['private server signup-token provisioning','real web password registration','CLI password login','create and decrypt synthetic login item','edit synthetic login item','private server sync','lock refuses item access','password unlock retains edited item','trash item and sync','logout'],scope='Disposable loopback compatibility server and actual CLI; initial web signup form, desktop UI, autofill, extension, biometrics, real backend and user feature inventory remain unverified',credentialsRetained=False)
finally:
 audited=cleanup(https,process,providers,guard,out)
result['retainedFilesAudited']=audited
result['serverWarnings']=[line for line in (out/'server.log').read_text().splitlines()if '[WARN]'in line]
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
