"""Disposable headless office fixture discovery; no display or transport listener."""
import base64,json,struct,subprocess,sys,zlib
from pathlib import Path
OUT=Path('/artifacts');PACKAGES=json.loads((OUT/'packages.json').read_text());report=[]
def run(app,label,args):
 result=subprocess.run([PACKAGES[app]+'/bin/'+app+'-cli',*args],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=60)
 (OUT/(label+'.stdout')).write_text(result.stdout);(OUT/(label+'.stderr')).write_text(result.stderr)
 report.append(dict(app=app,label=label,args=args,returncode=result.returncode));return result

def commands(items):return [value for name,data in items for value in ('--cmd',name+'='+json.dumps(data,separators=(',',':')))]
def chunk(name,data):return struct.pack('!I',len(data))+name+data+struct.pack('!I',zlib.crc32(name+data)&0xffffffff)
image=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('!2I5B',32,32,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+b'\xe0\x30\x50'*32)*32))+chunk(b'IEND',b'')
(OUT/'synthetic.png').write_bytes(image)
writer=[('text.insert',dict(text='Workflow heading')),('select.text',dict(text='Workflow heading')),('format.bold',dict(value=True)),('format.size',dict(size=18)),('caret.docEnd',{}),('text.newParagraph',{}),('text.insert',dict(text='Body text for a reversible edit.')),('select.text',dict(text='Body text')),('review.newComment',dict(text='Synthetic review comment')),('caret.docEnd',{}),('review.trackChanges',dict(value=True)),('text.insert',dict(text=' Tracked insertion.')),('review.trackChanges',dict(value=False)),('insert.header',dict(text='Synthetic header')),('insert.closeHeader',{}),('insert.footer',dict(text='Synthetic footer')),('insert.closeHeader',{}),('caret.docEnd',{}),('text.newParagraph',{}),('insert.link',dict(url='https://example.invalid/offline',text='Offline link')),('caret.docEnd',{}),('text.newParagraph',{}),('insert.picture',dict(data=base64.b64encode(image).decode(),width=48,alt='Synthetic image')),('caret.docEnd',{}),('layout.size',dict(name='A4')),('layout.margins',dict(top=54,bottom=54,left=54,right=54)),('layout.break',dict(kind='nextPage')),('text.insert',dict(text='Second section.')),('text.newParagraph',{}),('insert.table',dict(rows=2,cols=2)),('text.insert',dict(text='Table cell'))]
run('wordcraft','writer-create',['run',*commands(writer),'--author','Fixture','--save','/artifacts/writer-source.docx'])
for action in ('info','inspect'):run('wordcraft','writer-'+action,[action,'/artifacts/writer-source.docx'])
run('wordcraft','writer-export',['convert','/artifacts/writer-source.docx','/artifacts/writer.pdf'])
run('wordcraft','writer-odt',['convert','/artifacts/writer-source.docx','/artifacts/writer.odt'])
run('wordcraft','writer-odt-back',['convert','/artifacts/writer.odt','/artifacts/writer-from-odt.docx'])
grid=[('range.setValues',dict(range='A1',values=[['Item','Amount','Calculated'],['Gamma',30,'=B2*2'],['Alpha',10,'=B3*2'],['Beta',20,'=B4*2']])),('cell.set',dict(cell='E1',input='Total')),('cell.set',dict(cell='E2',input='=SUM(B2:B4)')),('cell.set',dict(cell='F1',input='Date')),('cell.set',dict(cell='F2',input='46315')),('home.formatCells',dict(range='F2',style=dict(numFmt='yyyy-mm-dd'))),('home.formatCells',dict(range='B2:C4',style=dict(numFmt='0.00'))),('data.sort',dict(range='A1:C4',header=True,keys=[dict(column='A',order='asc')])),('data.filter',dict(range='A1:C4')),('insert.chart',dict(range='A1:B4',type='column',title='Synthetic amounts',at='H2')),('pageLayout.printArea',dict(range='A1:L15')),('home.insertSheet',dict(name='Summary')),('cell.set',dict(cell='A1',input="='Sheet1'!E2")),('cell.set',dict(cell='B1',input='=LUMEN_UNSUPPORTED(1)'))]
run('gridcraft','grid-create',['run',*commands(grid),'--out','/artifacts/grid-source.xlsx'])
run('gridcraft','grid-info',['info','/artifacts/grid-source.xlsx','--json']);run('gridcraft','grid-values',['cat','/artifacts/grid-source.xlsx','--range','A1:F4','--sheet','Sheet1','--csv']);run('gridcraft','grid-summary',['cat','/artifacts/grid-source.xlsx','--sheet','Summary','--csv'])
run('gridcraft','grid-pdf',['run','--in','/artifacts/grid-source.xlsx',*commands([('file.exportPdf',dict(path='/artifacts/grid.pdf',sheets='all'))]),'--quiet'])
run('gridcraft','grid-ods',['convert','/artifacts/grid-source.xlsx','/artifacts/grid.ods'])
deck=[('file.new',dict(blank=True)),('slide.new',dict(layout='titleAndContent',title='Synthetic title',body='First body')),('insert.picture',dict(data=base64.b64encode(image).decode(),name='Synthetic image',rect=[400,100,64,64])),('shape.insert',dict(preset='roundRect',rect=[100,250,160,60],text='Synthetic shape')),('slide.notes',dict(text='Synthetic speaker notes')),('transition.set',dict(kind='fade',duration=600)),('slide.new',dict(layout='titleOnly',title='Second slide')),('insert.textBox',dict(rect=[100,150,300,80],text='Second slide content'))]
run('deckcraft','deck-create',['run',*commands(deck),'--save','/artifacts/deck-source.pptx'])
run('deckcraft','deck-info',['info','/artifacts/deck-source.pptx']);run('deckcraft','deck-pdf',['convert','/artifacts/deck-source.pptx','/artifacts/deck.pdf']);run('deckcraft','deck-odp',['convert','/artifacts/deck-source.pptx','/artifacts/deck.odp']);run('deckcraft','deck-odp-back',['convert','/artifacts/deck.odp','/artifacts/deck-from-odp.pptx'])
(OUT/'pilot.json').write_text(json.dumps(report,indent=2))
