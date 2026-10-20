"""Independent package facts for the deliberately small structured office corpus."""
import hashlib,posixpath
from pathlib import Path
from xml.etree import ElementTree as E
from zipfile import ZipFile,BadZipFile

W='http://schemas.openxmlformats.org/wordprocessingml/2006/main'
S='http://schemas.openxmlformats.org/spreadsheetml/2006/main'
A='http://schemas.openxmlformats.org/drawingml/2006/main'
P='http://schemas.openxmlformats.org/presentationml/2006/main'
R='http://schemas.openxmlformats.org/package/2006/relationships'
DOC_R='http://schemas.openxmlformats.org/officeDocument/2006/relationships'
def tag(space,name):return '{'+space+'}'+name
def local(value):return value.rsplit('}',1)[-1]
def attributes(element):return {local(k):v for k,v in sorted(element.attrib.items())} if element is not None else None
def texts(element,space):return ''.join(item.text or '' for item in element.iter(tag(space,'t')))
def xml(archive,name):return E.fromstring(archive.read(name))
def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def package_kind(path):
 if Path(path).read_bytes()[:5]==b'%PDF-':return 'pdf'
 try:
  with ZipFile(path) as z:
   names=z.namelist()
   if 'mimetype' in names:
    mime=z.read('mimetype').decode('ascii')
    return {'application/vnd.oasis.opendocument.text':'odt','application/vnd.oasis.opendocument.spreadsheet':'ods','application/vnd.oasis.opendocument.presentation':'odp','application/vnd.storyteller.deckcraft+zip':'deck-native'}.get(mime,'unknown-zip')
   for part,kind in [('word/document.xml','docx'),('xl/workbook.xml','xlsx'),('ppt/presentation.xml','pptx')]:
    if part in names and '[Content_Types].xml' in names:return kind
 except (OSError,ValueError,BadZipFile):pass
 return 'unknown'
def media(z,prefix):return sorted(hashlib.sha256(z.read(name)).hexdigest() for name in z.namelist() if name.startswith(prefix) and not name.endswith('/'))
def writer(path):
 with ZipFile(path) as z:
  doc=xml(z,'word/document.xml')
  paragraphs=[]
  for p in doc.iter(tag(W,'p')):
   runs=[]
   for r in p.iter(tag(W,'r')):
    props=r.find(tag(W,'rPr'))
    runs.append(dict(text=texts(r,W),format={local(c.tag):attributes(c) for c in props} if props is not None else {}))
   paragraphs.append(dict(text=texts(p,W),runs=runs))
  sections=[]
  for s in doc.iter(tag(W,'sectPr')):
   sections.append({local(c.tag):attributes(c) for c in s if local(c.tag) in ('pgSz','pgMar','cols','type')})
  notes=[dict(kind='header' if name.startswith('word/header') else 'footer',paragraphs=[texts(p,W) for p in xml(z,name).iter(tag(W,'p'))]) for name in z.namelist() if name.startswith(('word/header','word/footer')) and name.endswith('.xml')]
  comments=[]
  if 'word/comments.xml' in z.namelist():comments=[dict(author=c.get(tag(W,'author'),''),text=texts(c,W)) for c in xml(z,'word/comments.xml').iter(tag(W,'comment'))]
  links=[]
  if 'word/_rels/document.xml.rels' in z.namelist():links=sorted(c.get('Target') for c in xml(z,'word/_rels/document.xml.rels') if c.get('Type','').endswith('/hyperlink'))
  return dict(paragraphs=paragraphs,tables=[[[texts(c,W) for c in row.findall(tag(W,'tc'))] for row in table.findall(tag(W,'tr'))] for table in doc.iter(tag(W,'tbl'))],sections=sections,stories=sorted(notes,key=lambda item:(item['kind'],item['paragraphs'])),comments=comments,changes=[dict(kind=local(c.tag),author=c.get(tag(W,'author'),''),text=texts(c,W)) for c in doc.iter() if local(c.tag) in ('ins','del')],links=links,media=media(z,'word/media/'))
def spreadsheet(path):
 with ZipFile(path) as z:
  book=xml(z,'xl/workbook.xml');shared=[]
  if 'xl/sharedStrings.xml' in z.namelist():shared=[texts(c,S) for c in xml(z,'xl/sharedStrings.xml')]
  formats=[]
  if 'xl/styles.xml' in z.namelist():
   styles=xml(z,'xl/styles.xml');custom={c.get('numFmtId'):c.get('formatCode') for c in styles.findall('./'+tag(S,'numFmts')+'/'+tag(S,'numFmt'))}
   formats=[custom.get(c.get('numFmtId'),'builtin:'+c.get('numFmtId','0')) for c in styles.findall('./'+tag(S,'cellXfs')+'/'+tag(S,'xf'))]
  rels={c.get('Id'):c.get('Target') for c in xml(z,'xl/_rels/workbook.xml.rels')};sheets=[]
  for s in book.findall('./'+tag(S,'sheets')+'/'+tag(S,'sheet')):
   target=rels[s.get(tag(DOC_R,'id'))];name=target.lstrip('/') if target.startswith('/') else 'xl/'+target
   sheet=xml(z,name);cells={}
   for c in sheet.iter(tag(S,'c')):
    value=c.findtext(tag(S,'v'));kind=c.get('t','n')
    if kind=='s':value=shared[int(value)]
    elif kind=='inlineStr':value=texts(c,S)
    cells[c.get('r')]=dict(value=value,type=kind,formula=c.findtext(tag(S,'f')),numberFormat=formats[int(c.get('s','0'))] if formats else 'builtin:0')
   auto=sheet.find(tag(S,'autoFilter'))
   sheets.append(dict(name=s.get('name'),state=s.get('state','visible'),cells=cells,filter=E.tostring(auto,encoding='unicode') if auto is not None else None,hiddenRows=[c.get('r') for c in sheet.iter(tag(S,'row')) if c.get('hidden')=='1']))
  charts=[]
  for name in sorted(n for n in z.namelist() if n.startswith('xl/charts/chart') and n.endswith('.xml')):
   c=xml(z,name);charts.append(dict(text=texts(c,A),references=[e.text for e in c.iter() if local(e.tag)=='f'],types=[local(e.tag) for e in c.iter() if local(e.tag).endswith('Chart')]))
  return dict(sheets=sheets,charts=charts,names=[dict(name=c.get('name'),sheet=c.get('localSheetId'),value=c.text) for c in book.iter(tag(S,'definedName'))])
def presentation(path):
 with ZipFile(path) as z:
  root=xml(z,'ppt/presentation.xml');slides=[]
  names=sorted((n for n in z.namelist() if n.startswith('ppt/slides/slide') and n.endswith('.xml')),key=lambda n:int(Path(n).stem[5:]))
  for name in names:
   s=xml(z,name);layout=None;relation='ppt/slides/_rels/'+Path(name).name+'.rels'
   if relation in z.namelist():
    target=next((r.get('Target') for r in xml(z,relation) if r.get('Type','').endswith('/slideLayout')),None)
    if target:
     part=posixpath.normpath('ppt/slides/'+target) if not target.startswith('/') else target.lstrip('/');element=xml(z,part);content=element.find(tag(P,'cSld'));layout=dict(type=element.get('type'),name=content.get('name') if content is not None else None)
   slides.append(dict(text=texts(s,A),layout=layout,transitions=[dict(attributes=attributes(c),effects=[local(e.tag) for e in c]) for c in s.iter(tag(P,'transition'))],geometry=[dict(attributes=attributes(c),children=[dict(kind=local(e.tag),attributes=attributes(e)) for e in c]) for c in s.iter(tag(A,'xfrm'))]))
  notes=[texts(xml(z,n),A) for n in sorted(z.namelist()) if n.startswith('ppt/notesSlides/notesSlide') and n.endswith('.xml')]
  fonts=[]
  for name in sorted(n for n in z.namelist() if n.startswith('ppt/theme/theme') and n.endswith('.xml')):
   fonts.extend(dict(role=local(parent.tag),typeface=child.get('typeface')) for parent in xml(z,name).iter() if local(parent.tag) in ('majorFont','minorFont') for child in parent if local(child.tag)=='latin')
  return dict(slides=slides,notes=notes,size=attributes(root.find(tag(P,'sldSz'))),fonts=fonts,media=media(z,'ppt/media/'))
def differences(before,after,prefix=''):
 if type(before) is not type(after):return [dict(field=prefix,before=before,after=after)]
 if isinstance(before,dict):return [item for key in sorted(before.keys()|after.keys()) for item in differences(before.get(key),after.get(key),prefix+'.'+key if prefix else key)]
 if isinstance(before,list):
  if len(before)!=len(after):return [dict(field=prefix,before=before,after=after)]
  return [item for index,(a,b) in enumerate(zip(before,after)) for item in differences(a,b,prefix+'['+str(index)+']')]
 return [] if before==after else [dict(field=prefix,before=before,after=after)]
