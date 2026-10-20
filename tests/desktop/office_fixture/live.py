"""Read-only installed-provider projections for native save/reopen observations."""
import re

def without_runtime_ids(value):
 if isinstance(value,dict):return {key:without_runtime_ids(item) for key,item in value.items() if key!='id'}
 if isinstance(value,list):return [without_runtime_ids(item) for item in value]
 return value

def snapshot(control,app):
 if app=='writer':
  info=control.command(app,'file.info')
  document=control.tool('inspect_document')
  if not isinstance(document,dict):raise ValueError('Expected structured native document inspection')
  return dict(text=control.tool('get_text'),document=without_runtime_ids({key:value for key,value in document.items() if key not in ('selection','canUndo','canRedo')}),structure={key:info[key] for key in ['pages','paragraphs','sections','words','comments','trackChanges']})
 if app=='sheet':
  info=control.command(app,'document.inspect');sheets=[]
  for sheet in info['sheets']:
   name=sheet['name'];extent=sheet['usedRange']
   match=re.fullmatch(r'([A-Z]+)([1-9][0-9]*):([A-Z]+)([1-9][0-9]*)',extent)
   if not match:raise ValueError('Expected bounded synthetic sheet extent')
   def column(value):
    result=0
    for char in value:result=result*26+ord(char)-64
    return result
   first,last=column(match[1]),column(match[3]);top,bottom=int(match[2]),int(match[4])
   if first!=1 or top!=1 or last>26 or bottom>32:raise ValueError('Synthetic sheet exceeds native snapshot budget')
   cells={f'{chr(64+col)}{row}':control.tool('get_cell',dict(cell=name+'!'+f'{chr(64+col)}{row}')) for row in range(top,bottom+1) for col in range(first,last+1)}
   sheets.append(dict(metadata=without_runtime_ids(sheet),cells=cells,values=control.tool('read_range',dict(sheet=name,range=extent)),formulas=control.tool('read_range',dict(sheet=name,range=extent,formulas=True)),formatted=control.tool('read_range',dict(sheet=name,range=extent,formatted=True))))
  return dict(sheets=sheets)
 if app=='slides':
  info=control.command(app,'document.inspect')
  return without_runtime_ids(dict(fonts=info['fonts'],slideSize=info['slideSize'],slides=[control.tool('inspect_slide',dict(index=index)) for index in range(len(info['slides']))]))
 raise ValueError('Unknown installed provider')
