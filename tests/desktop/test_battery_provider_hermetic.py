"""Power fixture uses a real activation-free bus without ambient defaults."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


class BatteryProviderBus(unittest.TestCase):
    def test_real_provider_requires_explicit_private_configuration(self):
        fixture=Path(__file__).with_name('battery_provider.py')
        daemon=shutil.which('dbus-daemon')
        self.assertIsNotNone(daemon)
        with tempfile.TemporaryDirectory(prefix='lumen-power-regression-') as private:
            root=Path(private);bin=root/'bin';bin.mkdir()
            # Guard the invocation, then execute the actual daemon and provider.
            # This also fails on hosts where ambient defaults would mask the bug.
            guard=bin/'dbus-daemon'
            guard.write_text('#!'+sys.executable+'\n'+
                'import os,sys,xml.etree.ElementTree as ET\n'+
                'from pathlib import Path\n'+
                'assert "--session" not in sys.argv, "Ambient session configuration is forbidden"\n'+
                'config=next((x.split("=",1)[1] for x in sys.argv if x.startswith("--config-file=")),None)\n'+
                'assert config and not config.startswith("/etc/"), "Explicit private configuration required"\n'+
                'tree=ET.parse(config).getroot()\n'+
                'assert not any(tree.iter("servicedir")) and not any(tree.iter("include")) and not any(tree.iter("includedir"))\n'+
                'assert tree.findtext("listen").startswith("unix:tmpdir="), "Private runtime required"\n'+
                'os.execv('+repr(daemon)+',['+repr(daemon)+',*sys.argv[1:]])\n')
            guard.chmod(0o755)
            env=dict(os.environ,PATH=str(bin)+os.pathsep+os.environ['PATH'],HOME=private,
                     XDG_RUNTIME_DIR=private,DBUS_SESSION_BUS_ADDRESS='unix:path='+private+'/absent-session',
                     DBUS_SYSTEM_BUS_ADDRESS='unix:path='+private+'/absent-system',PYTHONWARNINGS='error')
            run=subprocess.run([sys.executable,str(fixture),'--output',str(root/'evidence')],env=env,
                               text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=30)
            self.assertEqual(run.returncode,0,run.stdout)
            result=json.loads((root/'evidence/result.json').read_text())
            self.assertTrue(result['passed']);self.assertTrue(result['activationFree'])
            self.assertTrue(result['explicitBusConfiguration']);self.assertEqual(len(result['checks']),10)
            self.assertNotIn('ResourceWarning',run.stdout)
            self.assertNotIn('Exception ignored',run.stdout)
            self.assertEqual(env['DBUS_SYSTEM_BUS_ADDRESS'],'unix:path='+private+'/absent-system')


if __name__=='__main__':unittest.main()
