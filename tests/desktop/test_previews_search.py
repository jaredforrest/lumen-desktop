import os,sys,tempfile,time,unittest,zipfile
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
from search import Index
from file_preview import preview
class SearchTests(unittest.TestCase):
 def test_folder_date_filters_and_filtered_ranking(self):
  import datetime
  with tempfile.TemporaryDirectory() as tmp:
   root=Path(tmp)/'files';(root/'Documents').mkdir(parents=True);(root/'Other').mkdir()
   for name,day in [('Documents/report.txt',10),('Documents/report old.txt',1),('Other/report.txt',10)]:
    file=root/name;file.write_text('sample');stamp=datetime.datetime(2026,1,day,12).timestamp();os.utime(file,(stamp,stamp))
   index=Index([root],Path(tmp)/'index.sqlite')
   try:
    index.scan()
    rows=index.search('report in:Documents since:2026-01-02 before:2026-02-01 ext:txt')['results']
    self.assertEqual([r['path'] for r in rows],[str(root/'Documents/report.txt')])
    self.assertEqual(index.search('report.txt ext:txt')['results'][0]['title'],'report.txt')
    self.assertFalse(index.search('in:Documents before:2025-01-01')['results'])
    with self.assertRaises(ValueError):index.search('since:2026-02-31')
   finally:index.closed.set();index.watcher.close()
 def test_events_scope_filters_and_previews(self):
  with tempfile.TemporaryDirectory() as t,patch.dict(os.environ,{'XDG_RUNTIME_DIR':t}):
   root=Path(t)/'files';root.mkdir();index=Index([root],Path(t)/'index.sqlite')
   try:
    (root/'example.txt').write_text('Plain text\n<b>literal</b>');(root/'Folder').mkdir()
    (root/'node_modules').mkdir();(root/'node_modules/hidden.txt').write_text('excluded')
    index.scan();time.sleep(.15)
    self.assertEqual(len(index.search('ext:txt')['results']),1)
    self.assertEqual(len(index.search('kind:folder')['results']),1)
    self.assertEqual(preview(root/'example.txt'),index.preview(str(root/'example.txt')))
    with self.assertRaises(ValueError):index.preview(str(root/'node_modules/hidden.txt'))
    with zipfile.ZipFile(root/'document.docx','w') as z:z.writestr('word/document.xml','<document><p><t>Office text</t></p></document>')
    self.assertEqual(preview(root/'document.docx')['text'],'Office text')
    (root/'new.txt').write_text('new')
    deadline=time.monotonic()+5
    while not index.search('new.txt')['results'] and time.monotonic()<deadline:time.sleep(.05)
    self.assertEqual(len(index.search('new.txt')['results']),1)
    (root/'new.txt').rename(root/'renamed.txt')
    deadline=time.monotonic()+5
    while not index.search('renamed.txt')['results'] and time.monotonic()<deadline:time.sleep(.05)
    self.assertEqual(len(index.search('renamed.txt')['results']),1)
    self.assertFalse(index.search('new.txt')['results'])
   finally:index.closed.set();index.watcher.close()
if __name__=='__main__':unittest.main()
