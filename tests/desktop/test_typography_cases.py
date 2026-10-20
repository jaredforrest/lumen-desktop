"""Segmentation must never omit or duplicate an accepted presentation case."""
import itertools,subprocess,sys,unittest
from pathlib import Path
from typography_cases import cases,identity,segments
class TypographyCases(unittest.TestCase):
 def test_eight_bounded_segments_preserve_all128_unique_cases(self):
  batches=segments();self.assertEqual(len(batches),8);all_cases=[]
  for batch in batches:
   rows=cases(**dict(consumer=batch['consumer'],width=batch['width'],mode=batch['mode']));self.assertEqual(len(rows),16);all_cases.extend(rows)
  expected={tuple(row)for consumer,views in [('Files',('list','grid','columns','gallery')),('Calendar',('Month','Year','Day','Week'))]for row in itertools.product([consumer],[480], [320],['light','dark'],[1,1.15,1.3,1.5],views)}
  expected|={tuple(row)for consumer,views in [('Files',('list','grid','columns','gallery')),('Calendar',('Month','Year','Day','Week'))]for row in itertools.product([consumer],[1060],[720],['light','dark'],[1,1.15,1.3,1.5],views)}
  actual=[identity(row)for row in all_cases];self.assertEqual(len(actual),128);self.assertEqual(len(set(actual)),128);self.assertEqual(set(actual),expected)
 def test_selected_segment_and_invalid_filters(self):
  self.assertEqual(segments('Calendar',1060,'dark'),[dict(consumer='Calendar',width=1060,mode='dark')])
  for values in [('Other',None,None),(None,999,None),(None,None,'other')]:
   with self.subTest(values=values),self.assertRaises(ValueError):segments(*values)
 def test_actual_entry_exposes_segment_argument_without_starting_qt(self):
  script=Path(__file__).with_name('owned_typography_qml.py');result=subprocess.run([sys.executable,str(script),'--help'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=5);self.assertEqual(result.returncode,0,result.stderr);self.assertIn('--color-mode',result.stdout)
if __name__=='__main__':unittest.main()
