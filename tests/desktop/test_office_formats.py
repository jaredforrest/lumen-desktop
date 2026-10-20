"""Package identity and independently parsed structured content never trust suffixes."""
import io,sys,tempfile,unittest
from pathlib import Path
from zipfile import ZipFile
sys.path.insert(0,str(Path(__file__).parent))
from office_fixture import corpus,formats as f
class Formats(unittest.TestCase):
 def archive(self,root,name,parts):
  path=root/name
  with ZipFile(path,'w') as archive:
   for key,value in parts.items():archive.writestr(key,value)
  return path
 def test_standard_container_identity_does_not_trust_success_or_suffix(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary)
   path=self.archive(root,'mislabelled.odp',{'mimetype':'application/vnd.storyteller.deckcraft+zip','presentation.json':'{}'})
   self.assertEqual(f.package_kind(path),'deck-native')
   self.assertEqual(f.package_kind(self.archive(root,'valid.odt',{'mimetype':'application/vnd.oasis.opendocument.text'})),'odt')
   path=root/'bad.docx';path.write_bytes(b'not a package');self.assertEqual(f.package_kind(path),'unknown')
 def test_writer_keeps_header_and_footer_roles_distinct(self):
  with tempfile.TemporaryDirectory() as temporary:
   doc='<w:document xmlns:w="'+f.W+'"><w:body><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Title</w:t></w:r><w:ins w:author="Fixture"><w:r><w:t>New</w:t></w:r></w:ins></w:p><w:sectPr><w:pgSz w:w="123"/></w:sectPr></w:body></w:document>'
   story=lambda name:'<w:hdr xmlns:w="'+f.W+'"><w:p><w:r><w:t>'+name+'</w:t></w:r></w:p></w:hdr>'
   path=self.archive(Path(temporary),'document.docx',{'[Content_Types].xml':'<Types/>','word/document.xml':doc,'word/header1.xml':story('Header'),'word/footer1.xml':story('Footer')})
   facts=f.writer(path);self.assertEqual(facts['paragraphs'][0]['text'],'TitleNew');self.assertEqual(facts['changes'][0]['author'],'Fixture');self.assertEqual(facts['sections'][0]['pgSz']['w'],'123')
   self.assertEqual(facts['stories'],[dict(kind='footer',paragraphs=['Footer']),dict(kind='header',paragraphs=['Header'])])
 def test_spreadsheet_formula_cached_error_and_format_are_separate(self):
  with tempfile.TemporaryDirectory() as temporary:
   path=self.archive(Path(temporary),'sheet.xlsx',{'[Content_Types].xml':'<Types/>','xl/workbook.xml':'<workbook xmlns="'+f.S+'" xmlns:r="'+f.DOC_R+'"><sheets><sheet name="Data" r:id="r1"/></sheets></workbook>','xl/_rels/workbook.xml.rels':'<Relationships><Relationship Id="r1" Target="worksheets/sheet1.xml"/></Relationships>','xl/worksheets/sheet1.xml':'<worksheet xmlns="'+f.S+'"><sheetData><row r="2" hidden="1"><c r="A2" t="e" s="1"><f>UNKNOWN()</f><v>#NAME?</v></c></row></sheetData><autoFilter ref="A1:A2"/></worksheet>','xl/styles.xml':'<styleSheet xmlns="'+f.S+'"><numFmts><numFmt numFmtId="164" formatCode="yyyy-mm-dd"/></numFmts><cellXfs><xf numFmtId="0"/><xf numFmtId="164"/></cellXfs></styleSheet>'})
   sheet=f.spreadsheet(path)['sheets'][0];self.assertEqual(sheet['cells']['A2'],dict(value='#NAME?',type='e',formula='UNKNOWN()',numberFormat='yyyy-mm-dd'));self.assertEqual(sheet['hiddenRows'],['2']);self.assertIn('A1:A2',sheet['filter'])
 def test_differences_include_typed_nested_losses(self):
  value=f.differences({'comments':[{'text':'Review'}],'count':1},{'comments':[],'count':True})
  self.assertEqual({row['field'] for row in value},{'comments','count'});self.assertEqual(value[0]['before'],[{'text':'Review'}])
 def test_corpus_image_and_action_payload_are_synthetic(self):
  from PIL import Image
  fixture=corpus.create()
  with Image.open(io.BytesIO(fixture['image'])) as image:self.assertEqual(image.size,(32,32));self.assertEqual(image.getpixel((0,0)),(224,48,80))
  self.assertTrue(all(isinstance(item,list) for key,item in fixture.items() if key!='image'))
if __name__=='__main__':unittest.main()
