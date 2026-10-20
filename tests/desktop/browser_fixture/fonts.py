"""Actual browser font selection and painting with explicit disposable inputs."""
import base64,hashlib,html,json,time
from pathlib import Path
from .protocol import PipeTransport


def audit(browser,environment,profile,output,cases):
    """Cases carry external font-policy expectations; this module supplies no fonts."""
    browser,profile,output=map(Path,(browser,profile,output))
    if not browser.is_file() or not str(browser).startswith('/nix/store/'):
        raise ValueError('Immutable browser executable required')
    if not profile.is_relative_to('/home/fixture') or '..' in profile.parts:
        raise ValueError('Explicit private browser profile required')
    if not output.is_relative_to('/artifacts') or '..' in output.parts or not output.is_dir():
        raise ValueError('Explicit artifact directory required')
    if not isinstance(cases,list) or not 1<=len(cases)<=64:
        raise ValueError('Bounded font cases required')
    names=set()
    for case in cases:
        if not isinstance(case,dict) or set(case)!={'name','family','text','expectedFamilies','expectedGlyphCount'}:
            raise ValueError('Explicit font case fields required')
        for key in ('name','family','text'):
            if not isinstance(case[key],str) or not 1<=len(case[key])<=1024:
                raise ValueError('Bounded font case text required')
        if case['name'] in names:raise ValueError('Unique font case names required')
        names.add(case['name'])
        families=case['expectedFamilies']
        if not isinstance(families,list) or not 1<=len(families)<=8 or any(not isinstance(f,str) or not 1<=len(f)<=256 for f in families):
            raise ValueError('Explicit expected font families required')
        if type(case['expectedGlyphCount']) is not int or not 1<=case['expectedGlyphCount']<=4096:
            raise ValueError('Explicit bounded glyph count required')
    profile.mkdir(mode=0o700)
    page='<html><head><meta charset="utf-8"><title>Font policy fixture</title></head><body style="background:#202124;color:#fff">'
    for index,case in enumerate(cases):
        page+='<div id="case-'+str(index)+'" style="font-size:32px;font-family:'+html.escape(case['family'],quote=True)+'">'+html.escape(case['text'])+'</div>'
    page+='</body></html>'
    command=[str(browser),'--user-data-dir='+str(profile),'--no-first-run','--no-default-browser-check','--headless','--disable-gpu','--remote-debugging-pipe','about:blank']
    with (output/'browser.log').open('w') as log,PipeTransport(command,environment,log) as transport:
        version=transport.request('Browser.getVersion',timeout=15)
        target=next(row for row in transport.request('Target.getTargets')['targetInfos'] if row['type']=='page')
        session=transport.request('Target.attachToTarget',dict(targetId=target['targetId'],flatten=True))['sessionId']
        def request(method,params=None):return transport.request(method,params,session=session)
        request('Page.enable');request('DOM.enable');request('CSS.enable')
        request('Page.navigate',dict(url='data:text/html;charset=utf-8;base64,'+base64.b64encode(page.encode()).decode()))
        deadline=time.monotonic()+10
        while True:
            ready=request('Runtime.evaluate',dict(expression="document.title==='Font policy fixture'&&document.readyState==='complete'",returnByValue=True))['result'].get('value')
            if ready:break
            if time.monotonic()>=deadline:raise TimeoutError('Synthetic font page readiness deadline')
            time.sleep(.02)
        request('Runtime.evaluate',dict(expression='document.fonts.ready.then(()=>true)',awaitPromise=True,returnByValue=True))
        document=request('DOM.getDocument')['root']['nodeId'];observations=[]
        for index,case in enumerate(cases):
            node=request('DOM.querySelector',dict(nodeId=document,selector='#case-'+str(index)))['nodeId']
            fonts=request('CSS.getPlatformFontsForNode',dict(nodeId=node))['fonts']
            observation=dict(case=case,fonts=fonts);observations.append(observation)
            (output/'observations.json').write_text(json.dumps(observations,ensure_ascii=False,indent=2)+'\n')
            if not fonts or any(f['familyName'] not in case['expectedFamilies'] or f['glyphCount']<=0 for f in fonts) or sum(f['glyphCount'] for f in fonts)!=case['expectedGlyphCount']:
                raise AssertionError(observation)
        image=base64.b64decode(request('Page.captureScreenshot',dict(format='png'))['data'],validate=True)
        if not image.startswith(b'\x89PNG\r\n\x1a\n'):raise AssertionError('Expected painted PNG')
        (output/'browser-fonts.png').write_bytes(image)
        result=dict(browser=str(browser),launcherSha256=hashlib.sha256(browser.read_bytes()).hexdigest(),version=version,command=command,observations=observations,passed=True,scope='Actual headless page font selection and screenshot; native chrome and external pages are outside this fixture')
        (output/'browser-fonts-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
        return result
