"""Native client fitting is idempotent, bounded and tied to one captured owner."""
import copy
import io
import json
import importlib.util
from pathlib import Path
import re
import sys
import socket
import tempfile
import threading
import unittest
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'))
spec=importlib.util.spec_from_file_location('window_geometry',Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime/window_geometry.py')
g=importlib.util.module_from_spec(spec);spec.loader.exec_module(g)


def tree(width=790,height=520):
    node=dict(id=5,pid=42,app_id='org.quickshell',name='Settings',type='floating_con',fullscreen_mode=0,scratchpad_state='none',rect=dict(x=50,y=100,width=width+2,height=height+1),window_rect=dict(x=1,y=0,width=width,height=height),deco_rect=dict(height=32),lumen_relationships=dict(parent=None,modal=None))
    return dict(type='root',nodes=[dict(type='workspace',name='1',rect=dict(x=0,y=28,width=960,height=692),floating_nodes=[node])])


class WindowGeometry(unittest.TestCase):
    def row(self,value=None):return g.exact_window(value or tree(),'org.quickshell','Settings',42)

    def test_fitting_native_window_is_a_true_noop(self):
        row=self.row();result,command=g.fit_plan(row,928,570,480,320)
        self.assertFalse(result['changed']);self.assertIsNone(command)
        self.assertEqual((result['clientWidth'],result['clientHeight']),(790,520))
        self.assertEqual((result['paddingWidth'],result['paddingHeight']),(2,33))

    def test_outer_budget_subtracts_real_decoration(self):
        result,command=g.fit_plan(self.row(tree(1200,900)),928,570,480,320)
        self.assertEqual((result['clientWidth'],result['clientHeight']),(926,537))
        self.assertEqual((result['outerWidth'],result['outerHeight']),(928,570))
        self.assertIn('resize set width 928 px height 570 px',command)

    def test_square_unified_content_has_no_invented_native_padding(self):
        row=self.row();row[0]['rect'].update(width=790,height=520);row[0]['deco_rect']['height']=0
        result,command=g.fit_plan(row,600,400,480,320)
        self.assertEqual((result['clientWidth'],result['clientHeight']),(600,400))
        self.assertEqual(result['paddingWidth'],0);self.assertEqual(result['paddingHeight'],0)

    def test_usable_workspace_and_minimum_size_are_both_explicit(self):
        row=self.row();row[1]['rect']['height']=400
        result,command=g.fit_plan(row,928,330,480,320)
        self.assertEqual(result['outerHeight'],353);self.assertTrue(result['minimumLimitsMargin'])
        row[1]['rect']['height']=300
        result,command=g.fit_plan(row,928,330,480,320)
        self.assertFalse(result['supported']);self.assertIsNone(command)

    def test_only_out_of_bounds_position_is_recentred(self):
        row=self.row();row[0]['rect']['x']=300
        result,command=g.fit_plan(row,928,570,480,320)
        self.assertTrue(result['recentred']);self.assertIn('move position center',command)
        self.assertNotIn('resize',command)

    def test_explicit_layout_and_owned_window_states_are_preserved(self):
        for change in [dict(type='con'),dict(fullscreen_mode=1),dict(lumen_pair=dict(id=2)),dict(lumen_state=dict(filled=True)),dict(lumen_relationships=dict(parent=9)),dict(lumen_relationships=dict(modal=True))]:
            row=self.row();row[0].update(change)
            result,command=g.fit_plan(row,500,400,480,320)
            self.assertFalse(result['supported']);self.assertIsNone(command)

    def test_owner_pid_app_title_and_container_must_all_match(self):
        for args in [('org.other','Settings',42,0),('org.quickshell','Other',42,0),('org.quickshell','Settings',43,0),('org.quickshell','Settings',42,6)]:
            with self.assertRaises(ValueError):g.exact_window(tree(),*args)
        value=tree();value['nodes'][0]['floating_nodes'].append(copy.deepcopy(value['nodes'][0]['floating_nodes'][0]))
        with self.assertRaises(ValueError):self.row(value)

    def test_literal_criterion_does_not_admit_another_title(self):
        title='A "quote"; [x] $(no) \\ café — Ω and\nline'
        expression=g.criterion(title)
        self.assertEqual(expression.count('"'),2)
        decoded=''.join(chr(int(value,16)) for value in re.findall(r'\\x\{([a-f0-9]+)\}',expression))
        self.assertEqual(decoded,title)
        self.assertTrue(expression.startswith('"'+r'\A'));self.assertTrue(expression.endswith(r'\z'+'"'))
        result,command=g.fit_plan(self.row(tree(1200,900)),928,570,480,320)
        self.assertIn('con_id=5 pid=42 app_id="',command);self.assertIn('title="',command)

    def test_same_title_replacement_cannot_be_retargeted(self):
        first=tree(1200,900);second=copy.deepcopy(first);second['nodes'][0]['floating_nodes'][0]['id']=6
        with patch.object(g.os,'getppid',return_value=42),patch.object(g,'process_start',return_value=99),patch.object(g,'socket_identity',return_value=(1,2,3)),patch.dict(g.os.environ,SWAYSOCK='/private/socket'),patch.object(g,'read_tree',side_effect=[first,second]),patch.object(g.subprocess,'check_output') as dispatch:
            with self.assertRaises(ValueError):g.execute('org.quickshell','Settings',0,928,570,480,320)
            dispatch.assert_not_called()

    def test_reused_pid_or_compositor_session_rejects_dispatch(self):
        for starts,sockets in [([99,100],[(1,2,3),(1,2,3)]),([99,99],[(1,2,3),(1,4,5)])]:
            with patch.object(g.os,'getppid',return_value=42),patch.object(g,'process_start',side_effect=starts),patch.object(g,'socket_identity',side_effect=sockets),patch.dict(g.os.environ,SWAYSOCK='/private/socket'),patch.object(g,'read_tree',side_effect=[tree(),tree()]),patch.object(g.subprocess,'check_output') as dispatch:
                with self.assertRaises(ValueError):g.execute('org.quickshell','Settings',0,928,570,480,320)
                dispatch.assert_not_called()

    def test_initial_map_wait_uses_acknowledged_private_events(self):
        errors=[]
        with tempfile.TemporaryDirectory() as private:
            address=str(Path(private)/'bus')
            with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as server:
                server.bind(address);server.listen(1)
                def publish():
                    try:
                        with server.accept()[0] as client:
                            kind,request=g.receive(client)
                            self.assertEqual((kind,request),(2,['window']))
                            g.send(client,2,'{"success":true}')
                            g.send(client,0x80000003,'{"change":"new","container":{"pid":42}}')
                    except BaseException as error:errors.append(error)
                thread=threading.Thread(target=publish);thread.start()
                with patch.dict(g.os.environ,SWAYSOCK=address),patch.object(g,'read_tree',side_effect=[{}, {},tree()]):
                    self.assertEqual(g.mapped_window('org.quickshell','Settings',42,0)[0]['id'],5)
                thread.join(timeout=2);self.assertFalse(thread.is_alive());self.assertEqual(errors,[])

    def test_native_margin_budget_does_not_use_stale_output_dimensions(self):
        row=self.row(tree(1060,720));row[1]['rect'].update(width=800,height=560)
        result,command=g.fit_plan(row,1888,1340,480,320,32,100)
        self.assertEqual((result['outerWidth'],result['outerHeight']),(768,460))
        self.assertEqual((result['clientWidth'],result['clientHeight']),(766,427))

    def test_observer_ack_resample_and_captured_workspace_changes(self):
        first=tree();first['nodes'][0]['id']=10
        moved=copy.deepcopy(first);moved['nodes'][0].update(id=11,rect=dict(x=960,y=0,width=800,height=560))
        replaced=copy.deepcopy(moved);replaced['nodes'][0]['floating_nodes'][0]['id']=6
        errors=[];output=io.StringIO()
        with tempfile.TemporaryDirectory() as private:
            address=str(Path(private)/'bus')
            with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as server:
                server.bind(address);server.listen(1)
                def publish():
                    try:
                        with server.accept()[0] as client:
                            self.assertEqual(g.receive(client),(2,['window','output']))
                            g.send(client,2,'{"success":true}')
                            for unused in range(3):g.send(client,0x80000003,'{"change":"move"}')
                    except BaseException as error:errors.append(error)
                thread=threading.Thread(target=publish);thread.start()
                with patch.dict(g.os.environ,SWAYSOCK=address),patch.object(g.os,'getppid',return_value=42),patch.object(g,'process_start',return_value=99),patch.object(g,'read_tree',side_effect=[first,moved,moved,replaced]),patch.object(g.sys,'stdout',output):
                    with self.assertRaises(EOFError):g.watch('org.quickshell','Settings',0)
                thread.join(timeout=2);self.assertFalse(thread.is_alive());self.assertEqual(errors,[])
        messages=[json.loads(line) for line in output.getvalue().splitlines()]
        self.assertEqual(messages[0],dict(ready=True))
        self.assertEqual([message['workspace'] for message in messages[1:]],[10,11])
        self.assertTrue(all(message['container']==5 and message['pid']==42 for message in messages[1:]))

    def test_retired_explicit_container_does_not_wait_for_replacement(self):
        with patch.object(g,'read_tree',return_value={}),patch.object(g.socket,'socket') as subscription:
            with self.assertRaises(ValueError):g.mapped_window('org.quickshell','Settings',42,5)
            subscription.assert_not_called()


if __name__=='__main__':unittest.main()
