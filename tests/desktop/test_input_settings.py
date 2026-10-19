"""Actual private compositor IPC plus synthetic devices and settings transport."""
import copy,importlib.util,json,os,subprocess,sys,tempfile,time,unittest
from unittest.mock import patch
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'res/lumen-desktop/shell'));sys.path.insert(0,str(Path(__file__).parent/'fixtures'))
import input_settings as inputs
from input_ipc import DEVICES,InputServer,settings_adapter


class InputPreferences(unittest.TestCase):
    def test_closed_values_and_scope(self):
        self.assertIsNone(inputs.current_value(dict(type='pointer',repeat_rate=25),'repeat_rate'))
        for key,value in [('repeat_rate',25),('repeat_delay',600),('pointer_accel',-.2),('scroll_factor',1.5),('tap',True)]:self.assertTrue(inputs.valid(key,value))
        for key,value in [('repeat_rate',True),('repeat_rate',1.2),('repeat_delay',99),('pointer_accel',float('nan')),('scroll_factor',0),('tap',1),('events','disabled')]:self.assertFalse(inputs.valid(key,value))
        for identifier in ('*','type:pointer','1:2:bad;exec','1:2:bad"','1:2:bad\n'):self.assertFalse(inputs.safe_identifier(identifier))

    def test_protocol_persistence_reversal_and_reconnect(self):
        original=dict(os.environ)
        with tempfile.TemporaryDirectory(prefix='lumen-input-provider-') as directory:
            root=Path(directory);os.environ.update(settings_adapter(root));os.environ['XDG_RUNTIME_DIR']=directory
            server=InputServer(root/'ipc.sock');os.environ['SWAYSOCK']=server.path
            try:
                def state():return inputs.handle(dict(action='status'))
                def device(kind):return next(row for row in state()['devices'] if row['identity']['type']==kind)
                def edit(kind,key,value):return inputs.handle(dict(action='set',device=device(kind)['key'],key=key,value=value))
                self.assertEqual(len(state()['devices']),3)
                self.assertNotIn('tap',[c['key'] for c in device('pointer')['controls']])
                before=copy.deepcopy(server.devices)
                edit('pointer','natural_scroll',True)
                self.assertEqual(server.devices[0],before[0]);self.assertEqual(server.devices[2],before[2]);self.assertEqual(server.devices[1]['libinput']['natural_scroll'],'enabled')
                self.assertEqual(server.actions[-1],['2:2:Fixture_Mouse','natural_scroll','enabled'])
                saved=json.loads((root/'settings.json').read_text())['inputs'];record=saved[device('pointer')['key']];self.assertIs(record['baseline']['natural_scroll'],False)
                edit('pointer','natural_scroll',False);self.assertIs(json.loads((root/'settings.json').read_text())['inputs'][device('pointer')['key']]['baseline']['natural_scroll'],False)
                edit('pointer','natural_scroll',True)
                restored=inputs.handle(dict(action='restore',device=device('pointer')['key'],key='natural_scroll'));self.assertEqual(server.devices[1]['libinput']['natural_scroll'],'disabled');self.assertFalse(device('pointer')['saved'])
                edit('pointer','pointer_accel',.3);server.devices[1]['libinput']['accel_speed']=.300000003
                self.assertTrue(next(c for c in device('pointer')['controls'] if c['key']=='pointer_accel')['inSync'])
                inputs.handle(dict(action='restore',device=device('pointer')['key'],key='pointer_accel'))
                original_save=inputs.set_section
                def activation_reset(section,value):
                    result=original_save(section,value);server.devices[0]['repeat_rate']=10;return result
                with patch.object(inputs,'set_section',side_effect=activation_reset):edit('keyboard','repeat_rate',40)
                self.assertEqual(server.devices[0]['repeat_rate'],40)
                inputs.handle(dict(action='restore',device=device('keyboard')['key'],key='repeat_rate'))
                (root/'settings.deny').touch()
                with self.assertRaises(ValueError):edit('keyboard','repeat_rate',40)
                self.assertEqual(server.devices[0]['repeat_rate'],25);self.assertEqual(json.loads((root/'settings.json').read_text())['inputs'],{})
                (root/'settings.deny').unlink();server.ignore=True
                with self.assertRaises(ValueError):edit('keyboard','repeat_delay',800)
                server.ignore=False
                def denied_rollback(section,value):server.reject=True;raise ValueError('Fixture save failed')
                with patch.object(inputs,'set_section',side_effect=denied_rollback):
                    with self.assertRaisesRegex(ValueError,'could not restore previous device value'):edit('keyboard','repeat_rate',40)
                server.devices[0]['repeat_rate']=25;server.reject=True
                with self.assertRaises(ValueError):edit('keyboard','repeat_rate',40)
                server.reject=False
                server.devices.append(copy.deepcopy(server.devices[1]))
                self.assertFalse(device('pointer')['editable'])
                with self.assertRaises(ValueError):edit('pointer','natural_scroll',True)
                server.devices.pop();server.devices[2]['libinput'].pop('tap')
                self.assertFalse(next(c for c in device('touchpad')['controls'] if c['key']=='tap')['supported'])
                with self.assertRaises(ValueError):edit('touchpad','tap',True)
                edit('pointer','natural_scroll',True)
                server.devices[1]['libinput']['natural_scroll']='disabled'
                self.assertFalse(next(c for c in device('pointer')['controls'] if c['key']=='natural_scroll')['inSync'])
                before_count=len(server.actions);original_ipc=inputs.ipc;first=[True]
                def duplicate_after_snapshot(kind,payload=''):
                    result=original_ipc(kind,payload)
                    if kind==100 and first[0]:first[0]=False;server.devices.append(copy.deepcopy(server.devices[1]))
                    return result
                with patch.object(inputs,'ipc',side_effect=duplicate_after_snapshot):inputs.restore_saved()
                self.assertEqual(len(server.actions),before_count);server.devices.pop()
                inputs.restore_saved();self.assertTrue(next(c for c in device('pointer')['controls'] if c['key']=='natural_scroll')['inSync'])
                saved_key=device('pointer')['key']
                watcher=subprocess.Popen([sys.executable,str(ROOT/'res/lumen-desktop/shell/input_settings.py'),'watch'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
                try:
                    end=time.monotonic()+4
                    while not server.subscribers and time.monotonic()<end:time.sleep(.02)
                    self.assertTrue(server.subscribers)
                    mouse=server.devices.pop(1);self.assertFalse(next(row for row in state()['devices'] if row['key']==saved_key)['connected'])
                    other=copy.deepcopy(mouse);other.update(identifier='4:4:Other_Mouse',name='Other Mouse');other['libinput']['natural_scroll']='disabled';server.add(other);time.sleep(.2);self.assertEqual(server.devices[-1]['libinput']['natural_scroll'],'disabled')
                    mouse['libinput']['natural_scroll']='disabled';server.add(mouse)
                    end=time.monotonic()+4
                    while server.devices[-1]['libinput']['natural_scroll']!='enabled' and time.monotonic()<end:time.sleep(.02)
                    self.assertEqual(server.devices[-1]['libinput']['natural_scroll'],'enabled');self.assertEqual(server.devices[-2]['libinput']['natural_scroll'],'disabled')
                finally:
                    watcher.terminate();stdout,stderr=watcher.communicate(timeout=5);self.assertEqual(stderr,'',stderr)
                result=inputs.handle(dict(action='forget',device=saved_key,confirmed=True));self.assertEqual(json.loads((root/'settings.json').read_text())['inputs'],{});self.assertEqual(server.devices[-1]['libinput']['natural_scroll'],'enabled')
            finally:server.close();os.environ.clear();os.environ.update(original)


if __name__=='__main__':unittest.main()
