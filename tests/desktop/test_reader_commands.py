"""Typed dispatch gates: exact owner, native lock revision, no retry or activation."""
import importlib.util
import os
from pathlib import Path
import unittest
source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime/reader_commands.py'
spec=importlib.util.spec_from_file_location('reader_commands',source);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

class Authority:
 def __init__(self):self.facts={'known':True,'locked':False};self.token=('private',);self.revision=1;self.calls=0;self.hook=None
 def refresh(self):
  self.calls+=1
  if self.hook:self.hook(self)

class Bus:
 def __init__(self):self.name=':1.3';self.uid=os.geteuid();self.pid=12;self.calls=[];self.requests=[];self.available=list(module.COMMANDS.values());self.list_hook=None;self.action_hook=None;self.owner_calls=0
 def get_object(self,name,path,introspect):
  self.requests.append((name,path,introspect));return self
 def GetNameOwner(self,name,**kwargs):self.calls.append(('owner',kwargs));self.owner_calls+=1;return self.name
 def GetConnectionUnixUser(self,name,**kwargs):self.calls.append(('uid',kwargs));return self.uid
 def GetConnectionUnixProcessID(self,name,**kwargs):self.calls.append(('pid',kwargs));return self.pid
 def ListCommands(self,**kwargs):
  if self.list_hook:self.list_hook(self)
  return [(action,'Synthetic description') for _,action in self.available]
 def ExecuteCommand(self,action,notify,**kwargs):
  self.calls.append(('execute',action,notify,kwargs))
  if self.action_hook:self.action_hook(self)
  return True

class Commands(unittest.TestCase):
 def fixture(self):
  bus=Bus();authority=Authority();client=module.ReaderCommands(bus,authority,identity=lambda pid:100);return bus,authority,client
 def rejected(self,client,command,reason):
  with self.assertRaises(module.Unavailable) as result:client.execute(command)
  self.assertEqual(str(result.exception),reason)
 def test_allowlist_unique_owner_and_bounded_nonactivating_calls(self):
  bus,authority,client=self.fixture();self.assertTrue(client.execute('where-am-i')['accepted'])
  executed=[row for row in bus.calls if row[0]=='execute'];self.assertEqual(len(executed),1);self.assertEqual(executed[0][1:3],('WhereAmIBasic',False))
  self.assertEqual(bus.requests[1],(':1.3',module.BASE+'WhereAmIPresenter',False))
  for row in bus.calls:self.assertGreater(row[-1]['timeout'],0);self.assertLessEqual(row[-1]['timeout'],2.5)
  self.rejected(client,'arbitrary','unsupported_command');self.assertEqual(len([row for row in bus.calls if row[0]=='execute']),1)
 def test_locked_unknown_and_revision_change_prevent_reads(self):
  for facts in ({'known':True,'locked':True},{'known':False,'locked':True}):
   bus,authority,client=self.fixture();authority.facts=facts;self.rejected(client,'read-all','session_unavailable');self.assertEqual(bus.calls,[])
  bus,authority,client=self.fixture();authority.hook=lambda value:setattr(value,'revision',value.calls);self.rejected(client,'read-all','source_changed');self.assertFalse(any(row[0]=='execute' for row in bus.calls))
 def test_safe_stop_works_without_any_lock_authority(self):
  bus,authority,client=self.fixture();authority.facts={'known':False,'locked':True};self.assertTrue(client.execute('stop')['accepted']);self.assertEqual(authority.calls,0)
 def test_owner_replacement_during_listing_prevents_dispatch(self):
  bus,authority,client=self.fixture();bus.list_hook=lambda value:setattr(value,'name',':1.4');self.rejected(client,'where-am-i','source_changed');self.assertFalse(any(row[0]=='execute' for row in bus.calls))
 def test_foreign_owner_and_unsupported_provider_never_dispatch(self):
  bus,authority,client=self.fixture();bus.uid=os.geteuid()+1;self.rejected(client,'where-am-i','reader_unavailable')
  bus,authority,client=self.fixture();bus.available=[];self.rejected(client,'where-am-i','unsupported_command');self.assertFalse(any(row[0]=='execute' for row in bus.calls))
 def test_changed_owner_after_ack_is_ambiguous_and_never_retried(self):
  bus,authority,client=self.fixture();bus.action_hook=lambda value:setattr(value,'name',':1.4');self.rejected(client,'where-am-i','dispatch_state_changed');self.assertEqual(len([row for row in bus.calls if row[0]=='execute']),1)
 def test_elapsed_budget_prevents_dispatch(self):
  bus,authority,client=self.fixture();client.clock=lambda:client.deadline+1;self.rejected(client,'where-am-i','provider_timeout');self.assertFalse(any(row[0]=='execute' for row in bus.calls))
if __name__=='__main__':unittest.main()
