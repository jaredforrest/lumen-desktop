"""Deployment changes must not leave actions targeting an obsolete asset path."""
import importlib.util
from pathlib import Path
import os
import subprocess
import unittest
from unittest.mock import patch

MODULE=Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime/shell_ipc.py'
spec=importlib.util.spec_from_file_location('shell_ipc',MODULE)
ipc=importlib.util.module_from_spec(spec);spec.loader.exec_module(ipc)

class ShellIpcTests(unittest.TestCase):
    def test_selects_service_pid_not_build_asset_path(self):
        calls=[]
        def run(command,**kwargs):
            calls.append(command)
            return subprocess.CompletedProcess(command,0,stdout=str(os.getpid())+'\n')
        with patch.object(ipc.subprocess,'run',side_effect=run):
            self.assertEqual(ipc.main(['--','call','desktop','appSwitcher','next']),0)
        self.assertEqual(calls[1],['quickshell','ipc','--pid',str(os.getpid()),'call','desktop','appSwitcher','next'])
        self.assertNotIn('-p',calls[1])

    def test_inactive_and_malformed_service_pid_never_dispatch(self):
        for value in ['0','', '123\n456','-1','１２３']:
            with self.subTest(value=value), patch.object(ipc.subprocess,'run',return_value=subprocess.CompletedProcess([],0,stdout=value)) as run:
                self.assertEqual(ipc.main(['--','call','desktop','appSwitcher','next']),1)
                self.assertEqual(run.call_count,1)

    def test_exited_pid_and_restart_race_do_not_replay_action(self):
        with patch.object(ipc.subprocess,'run',return_value=subprocess.CompletedProcess([],0,stdout='999999999')), patch.object(ipc.Path,'stat',side_effect=FileNotFoundError):
            self.assertEqual(ipc.main(['--','call','desktop','appSwitcher','next']),1)
        with patch.object(ipc.subprocess,'run',side_effect=[subprocess.CompletedProcess([],0,stdout=str(os.getpid())),subprocess.CompletedProcess([],1)]) as run:
            self.assertEqual(ipc.main(['--','call','desktop','appSwitcher','next']),1)
            self.assertEqual(run.call_count,2)

if __name__=='__main__':unittest.main()
