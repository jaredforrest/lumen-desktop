"""Deterministic coupled-context planning without a compositor or user files."""
import copy,json,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'))
from display_space_policy import display_identity,identity,primary,plan,preferences,identity_record

def output(name,x=0,focused=False):return dict(name=name,make='',model='',serial='',active=True,focused=focused,rect=dict(x=x,y=0))
def desktop(key,name,visible=False):return dict(logicalId=key,outputId='connector:'+name,visible=visible,name=key)

class Policy(unittest.TestCase):
 def test_typed_identity(self):
  key=display_identity(dict(name='A',make='Maker',model='Panel',serial='123'));self.assertEqual(identity(key),key)
  for value in (None,'','connector:','connector:A\n','display:["x","y","0"]','display:["x", "y", "1"]'):
   with self.assertRaises(ValueError):identity(value)
 def test_typed_preferences_and_unicode_identity(self):
  record=dict(kind='display',name=None,make='Maker',model='Écran',serial='abc')
  chosen=preferences(dict(separateDisplays=False,primaryDisplay=record));self.assertFalse(chosen['separateDisplays']);self.assertEqual(identity_record(chosen['primaryDisplay']),record)
  self.assertEqual(preferences({}),dict(separateDisplays=True,primaryDisplay=None))
  for value in (dict(separateDisplays=1),dict(primaryDisplay='connector:A'),dict(primaryDisplay=dict(kind='connector',name='A',serial='1')),dict(primaryDisplay=dict(kind='display',make='M',model='P',serial='Unknown'))):
   with self.assertRaises(ValueError):preferences(value)
 def test_primary_is_not_focus(self):
  rows=[output('right',100,True),output('left',0)]
  self.assertEqual(primary(None,None,rows)['name'],'left')
  first=primary('connector:right',None,rows);self.assertEqual(first['name'],'right')
  missing=primary(first['preferred'],None,[rows[1]]);self.assertTrue(missing['fallback']);self.assertEqual(missing['preferred'],'connector:right')
  self.assertEqual(primary(missing['preferred'],None,rows)['name'],'right')
 def test_explicit_connector_remains_resolvable_when_metadata_becomes_available(self):
  rows=[output('A'),dict(output('B',100),make='Maker',model='Panel',serial='123')]
  result=primary('connector:B',None,rows);self.assertEqual(result['name'],'B');self.assertFalse(result['fallback']);self.assertTrue(result['effective'].startswith('display:'))
 def test_primary_ambiguity(self):
  with self.assertRaises(ValueError):primary(None,None,[output('A'),output('A')])
 def test_first_toggle_preserves_visible_composition(self):
  rows=[desktop('a1','A'),desktop('a2','A',True),desktop('b1','B',True)]
  original=copy.deepcopy(rows);ids=iter(['cohort1','cohort2','new-desktop'])
  result=plan([],rows,[output('A'),output('B')],preserve_visible=True,new_id=lambda:next(ids))
  self.assertEqual(rows,original);self.assertEqual(result['cohorts'][0]['members'],{'connector:A':'a2','connector:B':'b1'})
  self.assertEqual(len(result['created']),1);self.assertEqual(result['cohorts'][1]['members']['connector:A'],'a1')
 def test_reconcile_stable(self):
  old=[dict(id='one',members={'connector:A':'a','connector:B':'b'})];rows=[desktop('a','A',True),desktop('b','B',True)]
  self.assertEqual(plan(old,rows,[output('A'),output('B')])['cohorts'],old)
 def test_output_loss_repackages_all_migrated_desktops(self):
  old=[dict(id='one',members={'connector:A':'a','connector:B':'b'})];rows=[desktop('a','A',True),desktop('b','A')]
  result=plan(old,rows,[output('A')],new_id=lambda:'new-context')
  self.assertEqual({c['members']['connector:A'] for c in result['cohorts']},{'a','b'});self.assertEqual(result['created'],[]);self.assertEqual(result['provenance'],old)
 def test_capacity_refusal_leaves_inputs_untouched(self):
  rows=[desktop(str(n),'A',n==0) for n in range(63)]+[desktop('b','B',True)];before=copy.deepcopy(rows)
  ids=iter('context'+str(n) for n in range(64))
  with self.assertRaisesRegex(ValueError,'budget'):plan([],rows,[output('A'),output('B')],new_id=lambda:next(ids))
  self.assertEqual(rows,before)
 def test_duplicate_visible_refused(self):
  with self.assertRaises(ValueError):plan([],[desktop('a','A',True),desktop('b','A',True)],[output('A')],preserve_visible=True)
 def test_retained_members_can_follow_a_gap(self):
  old=[dict(id='c'+str(n),members={'connector:Z'+str(n):'d'+str(n)}) for n in range(4)]
  rows=[desktop('a','A',True)]+[desktop('d'+str(n),'Z'+str(n)) for n in range(4)]
  result=plan(old,rows,[output('A')]);self.assertEqual({value for row in result['cohorts'] for value in row['members'].values()},{row['logicalId'] for row in rows})
 def test_duplicate_generated_cohort_refused(self):
  with self.assertRaisesRegex(ValueError,'generated cohort'):plan([],[desktop('a','A',True),desktop('b','A')],[output('A')],new_id=lambda:'same')
 def test_no_outputs_prunes_retired_members(self):
  old=[dict(id='one',members={'connector:A':'retired'})]
  self.assertEqual(plan(old,[],[])['cohorts'],[])
 def test_disconnected_member_retained(self):
  old=[dict(id='one',members={'connector:A':'a','connector:B':'b'})]
  result=plan(old,[desktop('a','A',True),desktop('b','B')],[output('A')]);self.assertEqual(result['cohorts'],old)

if __name__=='__main__':unittest.main()
