import unittest
from office_fixture.live import snapshot,without_runtime_ids
class Live(unittest.TestCase):
 def test_only_opaque_id_is_removed(self):
  original={'id':1,'shapeId':9,'text':'sample','box':[1,2,3,4],'fill':{'id':2,'color':'red'}}
  self.assertEqual(without_runtime_ids(original),{'shapeId':9,'text':'sample','box':[1,2,3,4],'fill':{'color':'red'}})
  self.assertEqual(original['id'],1)
 def test_writer_projection_contains_all_text_and_declared_counts(self):
  class Reader:
   def command(self,app,name):return dict(pages=2,paragraphs=10,sections=2,words=20,comments=1,trackChanges=False,path='ignored',dirty=False)
   def tool(self,name):
    return {'blocks':[{'text':'all body text','runs':[{'bold':True}]}],'selection':{'offset':2},'canUndo':True,'canRedo':False} if name=='inspect_document' else {'body':'all body text','header':'header text'}
  result=snapshot(Reader(),'writer');self.assertEqual(result['text']['header'],'header text');self.assertEqual(result['structure']['comments'],1);self.assertNotIn('path',result['structure']);self.assertEqual(result['document'],{'blocks':[{'text':'all body text','runs':[{'bold':True}]}]})
 def test_sheet_bounds_fail_before_any_cell_read(self):
  class Reader:
   def command(self,app,name):return {'sheets':[{'name':'Sheet1','usedRange':'A1:ZZ999'}]}
   def tool(self,*args):raise AssertionError('Unbounded cell access')
  with self.assertRaises(ValueError):snapshot(Reader(),'sheet')
if __name__=='__main__':unittest.main()
