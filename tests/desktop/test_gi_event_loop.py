"""Modern GLib loop factories stay quiet; explicit legacy calls still diagnose."""
import subprocess
import sys
import unittest


class GLibAsyncioCompatibility(unittest.TestCase):
    def check_program(self,program):
        result=subprocess.run([sys.executable,'-W','error','-c',program],text=True,capture_output=True,timeout=15)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)

    def test_modern_runner_does_not_load_deprecated_policy(self):
        self.check_program('''import asyncio
import gi.events as events
assert "GLibEventLoopPolicy" not in vars(events)
async def value():
    await asyncio.sleep(0)
    return 23
with asyncio.Runner(loop_factory=events.GLibEventLoop) as runner:
    assert runner.run(value()) == 23
from gi.repository import GLib
main = GLib.MainLoop()
def quit():
    main.quit()
    return False
GLib.idle_add(quit)
main.run()
assert "GLibEventLoopPolicy" not in vars(events)
''')

    def test_explicit_legacy_policy_remains_compatible_and_deprecated(self):
        self.check_program('''import asyncio, warnings
import gi.events as events
assert "GLibEventLoopPolicy" in dir(events)
async def value():
    await asyncio.sleep(0)
    return 41
with warnings.catch_warnings(record=True) as diagnostics:
    warnings.simplefilter("always", DeprecationWarning)
    policy = events.GLibEventLoopPolicy
    assert policy.__qualname__ == "GLibEventLoopPolicy"
    with policy():
        assert asyncio.get_event_loop().run_until_complete(value()) == 41
assert diagnostics
assert any("AbstractEventLoopPolicy" in str(item.message) for item in diagnostics)
assert all(issubclass(item.category, DeprecationWarning) for item in diagnostics)
''')


if __name__=='__main__':unittest.main()
