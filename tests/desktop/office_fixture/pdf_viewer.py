"""Explicit private PDF association to the retained browser, without host handlers."""
import hashlib,os,shlex,shutil
from pathlib import Path

def configure(environment,browser,xdg_tools,mime_data):
 browser=Path(browser).resolve(strict=True);xdg_tools=Path(xdg_tools).resolve(strict=True);mime_data=Path(mime_data).resolve(strict=True)
 if not str(mime_data).startswith('/nix/store/') or not (mime_data/'mime/mime.cache').is_file():raise ValueError('Pinned MIME catalogue required')
 if not str(browser).startswith('/nix/store/') or not str(xdg_tools).startswith('/nix/store/') or not (xdg_tools/'bin/xdg-open').is_file():raise ValueError('Pinned browser and XDG utilities required')
 applications=Path(environment['XDG_DATA_HOME'])/'applications';applications.mkdir(parents=True,exist_ok=True)
 shell=shutil.which('bash',path=environment['PATH']);assert shell and shell.startswith('/nix/store/')
 helper=Path('/home/fixture/pdf-viewer');script='#!'+shell+'\nexec >>/artifacts/pdf-viewer.log 2>&1\nprintf \"viewer-start %s\\n\" \"$$\"\nunset GTK_USE_PORTAL XDG_CURRENT_DESKTOP\nsleep 0.4\nexec '+shlex.quote(str(browser))+' --user-data-dir=/home/fixture/pdf-profile --no-first-run --no-default-browser-check --ozone-platform=wayland --force-renderer-accessibility "$@"\n';helper.write_text(script);helper.chmod(0o700)
 (applications/'fixture-pdf.desktop').write_text('[Desktop Entry]\nType=Application\nName=Fixture PDF Viewer\nExec='+str(helper)+' %U\nMimeType=application/pdf;\nTerminal=false\n')
 (Path(environment['XDG_CONFIG_HOME'])/'mimeapps.list').write_text('[Default Applications]\napplication/pdf=fixture-pdf.desktop\n')
 environment['XDG_DATA_DIRS']=str(mime_data)+':'+environment.get('XDG_DATA_DIRS','')
 environment['PATH']=str(xdg_tools/'bin')+':'+environment['PATH'];environment['BROWSER']=str(helper)
 return dict(browser=str(browser),sha256=hashlib.sha256(browser.read_bytes()).hexdigest(),helper=str(helper),helperSha256=hashlib.sha256(script.encode()).hexdigest(),xdgTools=str(xdg_tools),delaySeconds=.4,scope='Private association and delayed real browser process only; no fake viewer or printer')
