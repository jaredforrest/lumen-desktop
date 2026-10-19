"""Focus policy boundaries, scheduling, expiry and restart without a live desktop."""
import datetime
import importlib.util
import json
from pathlib import Path
import tempfile
from unittest.mock import patch

source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/notifications/focus.py'
spec=importlib.util.spec_from_file_location('focus_policy',source)
f=importlib.util.module_from_spec(spec);spec.loader.exec_module(f)
with tempfile.TemporaryDirectory() as directory:
 path=Path(directory)/'focus.json'; engine=f.Focus(path)
 assert not engine.state()['active'] and engine.allows('chat',1)
 engine.command({'profile':'work'})
 assert not engine.allows('chat',1) and not engine.allows('chat',2)
 config=dict(id='work',name='Work',allowedApps=['calendar'],allowCritical=True,schedules=[dict(days=[0],start=22*60,end=6*60)])
 engine.command({'operation':'profile','profile':config})
 assert engine.allows('calendar',1) and engine.allows('chat',2) and not engine.allows('chat',1)
 assert f.scheduled(config,datetime.datetime(2026,10,19,23))
 assert f.scheduled(config,datetime.datetime(2026,10,20,5,59))
 assert not f.scheduled(config,datetime.datetime(2026,10,20,6))
 assert not f.scheduled(config,datetime.datetime(2026,10,21,1))
 assert not f.scheduled(config,datetime.datetime(2026,10,19,21,59))
 config['schedules']=[dict(days=[0],start=540,end=1020)]
 assert f.scheduled(config,datetime.datetime(2026,10,19,9))
 assert not f.scheduled(config,datetime.datetime(2026,10,19,17))
 engine.command({'profile':'work','seconds':3600})
 assert path.stat().st_mode&0o777==0o600
 restored=f.Focus(path);assert restored.state()['active']=='work'
 # Wall clock forwards and suspend-aware monotonic expiry independently clear.
 with patch.object(f.time,'time',return_value=engine.until+1):
  state=engine.state()
  assert state['scheduled'] and state['until']==0 and engine.manual is None
 engine.command({'profile':'work','seconds':3600})
 with patch.object(engine,'boot_time',return_value=engine.boot_deadline+1),patch.object(f.time,'time',return_value=1):
  state=engine.state()
  assert state['scheduled'] and state['until']==0 and engine.manual is None
 # Expiry restores automatic scheduling, including a currently matching profile.
 engine.command({'profile':'quiet','seconds':3600})
 with patch.object(f.time,'time',return_value=engine.until+1),patch.object(f,'scheduled',side_effect=lambda item,now:item['id']=='work'):
  assert engine.state()['active']=='work' and engine.manual is None
 engine.command({'profile':''});assert engine.state()['active']=='' and not engine.state()['scheduled']
 engine.command({'profile':None});assert engine.state()['scheduled']
 for bad in [dict(config,allowCritical='yes'),dict(config,allowedApps=['']),dict(config,schedules=[dict(days=[True],start=1,end=2)]),dict(config,schedules=[dict(days=[1],start=2,end=2)]),dict(config,schedules=[dict(days=[1],start=1,end=1440)])]:
  try:f.profile(bad)
  except ValueError:pass
  else:raise AssertionError('Accepted invalid profile')
 for duration in [-1,True,86401,1.5]:
  try:engine.command({'profile':'work','seconds':duration})
  except ValueError:pass
  else:raise AssertionError('Accepted invalid duration')
 path.write_text('{bad');recovered=f.Focus(path)
 assert not path.exists() and list(Path(directory).glob('focus-invalid-*.json'))
print('PASS Focus profiles, exceptions, critical policy, daily/overnight boundaries, sleep/clock expiry, restart, privacy and validation')
