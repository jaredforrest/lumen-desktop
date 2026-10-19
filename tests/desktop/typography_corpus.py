"""Qt glyph-run and painted typography corpus; no claim of reference metrics."""
import argparse
from collections import Counter
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess as sp
import tempfile

p=argparse.ArgumentParser();p.add_argument('--probe',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--family',default='Interlude Variable');a=p.parse_args()
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
spec=importlib.util.spec_from_file_location('corpus_palette',repo/'res/lumen-desktop/appearance/appearance.py');appearance=importlib.util.module_from_spec(spec);spec.loader.exec_module(appearance)
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text())
cases=[
    dict(name='Ordinary time and digits',text='00:00  09:41  12:34:56  23:59:59  0123456789',size=24),
    dict(name='Tabular digits',text='000000 111111 222222 333333 444444 555555 666666 777777 888888 999999',size=20,tabular=True),
    dict(name='Title',text='Jérôme’s résumé — Søren, Łódź, Zürich & İstanbul',size=26),
    dict(name='Menu and punctuation',text='File  Edit  View  “Open…”  Save As…  ⌘ ⌥ ⇧ ⌃ ↩ ⎋ • — – ([]{})',size=20),
    dict(name='Message and accented names',text='São Paulo · François · Dvořák · Smørrebrød · naïve · Ångström',size=20),
    dict(name='Greek and Cyrillic',text='Καλημέρα κόσμε · Ελληνικά · Добро пожаловать · Кириллица',size=22),
    dict(name='Arabic and Latin numerals',text='مرحبا بالعالم — العربية ١٢٣٤٥٦٧٨٩٠ / 09:41',size=24),
    dict(name='Devanagari',text='नमस्ते दुनिया · हिन्दी · १२३४५६७८९०',size=24),
    dict(name='Japanese and Chinese',text='日本語の名前 · 中文字符 · 2026年10月19日',size=24),
    dict(name='Korean',text='안녕하세요 · 한국어 · 2026년 10월 19일',size=24),
    dict(name='Explicit text presentation',text='1\ufe0e 2\ufe0e #\ufe0e *\ufe0e 0123456789',size=24),
    dict(name='Explicit emoji presentation',text='😀 🌍 🇬🇧 👍🏽 👩\u200d💻 1\ufe0f\u20e3',size=30,emoji=True),
]
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
with tempfile.TemporaryDirectory(prefix='lumen-typography-') as private:
    env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,QT_QPA_PLATFORM='offscreen',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
    for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
    for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
    exported=sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env)
    env.update(dict(v.decode().split('=',1) for v in exported.split(b'\0') if b'=' in v))
    env.pop('QT_PLUGIN_PATH',None)
    env.pop('QML_IMPORT_PATH',None)
    env.update(QT_QPA_PLATFORMTHEME='generic',QT_QPA_PLATFORM='offscreen',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
    from PIL import Image,ImageStat
    results=[]
    for mode in ('light','dark'):
        palette=appearance.resolve_palette(palettes,mode,appearance.DEFAULTS)
        for scale in (1,2):
            config=dict(family=a.family,background=palette['background'],foreground=palette['text'],muted=palette['muted'],scale=scale,cases=cases)
            path=out/f'{mode}-{scale}x.json';path.write_text(json.dumps(config,ensure_ascii=False))
            run=sp.run([str(a.probe),str(path),str(out/f'{mode}-{scale}x.png')],env=env,text=True,capture_output=True,timeout=30)
            (out/f'{mode}-{scale}x.log').write_text(run.stderr)
            assert run.returncode==0,run.stderr
            assert 'warning' not in run.stderr.lower(),run.stderr
            data=json.loads(run.stdout)
            for requested,shaped in zip(cases,data['cases']):
                assert shaped['name']==requested['name']
                assert shaped['missingGlyphs']==0,shaped
                assert shaped['runs'] and shaped['width']>0,shaped
                families=[entry['family'] for entry in shaped['runs']]
                emoji=any('emoji' in family.lower() for family in families)
                assert emoji==requested.get('emoji',False),(requested['name'],families)
                if requested.get('tabular'):
                    assert max(shaped['digitWidths'])-min(shaped['digitWidths'])<.02,shaped
            image=Image.open(out/f'{mode}-{scale}x.png').convert('RGB')
            for index,item in enumerate(cases):
                crop=image.crop((24*scale,(37+index*76)*scale,1376*scale,(75+index*76)*scale))
                assert max(ImageStat.Stat(crop).stddev)>2,('Unpainted corpus row',item['name'])
            results.append(dict(mode=mode,**data))
    (out/'result.json').write_text(json.dumps(dict(passed=True,renderer='Qt QTextLayout software',scope='Glyph ownership, coverage, tabular widths and painted corpus; not reference typography equivalence',results=results),ensure_ascii=False,indent=2)+'\n')
print('PASS typography corpus: light/dark at 1x/2x, no missing glyphs or ordinary-digit emoji fallback')
