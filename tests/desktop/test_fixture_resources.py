"""Fixture subprocess descriptors remain usable until reaped, then close explicitly."""
import ast
from pathlib import Path
import subprocess
import sys
import unittest
from fixture_resources import close_reaped_pipes

class FixtureResources(unittest.TestCase):
    def test_native_log_handles_have_explicit_owner(self):
        for path in Path(__file__).parent.glob('*.py'):
            for node in ast.walk(ast.parse(path.read_text())):
                if not (isinstance(node,ast.Call) and isinstance(node.func,ast.Attribute) and node.func.attr=='Popen'):continue
                for keyword in node.keywords:
                    value=keyword.value
                    self.assertFalse(isinstance(value,ast.Call) and ((isinstance(value.func,ast.Attribute) and value.func.attr=='open') or (isinstance(value.func,ast.Name) and value.func.id=='open')),f'{path.name}:{node.lineno}: Popen must borrow a context-owned log, not an anonymous open handle')
    def test_named_log_handles_are_context_or_cleanup_owned(self):
        for path in Path(__file__).parent.glob('*.py'):
            tree=ast.parse(path.read_text());opened={}
            for node in ast.walk(tree):
                if isinstance(node,ast.Assign) and isinstance(node.value,ast.Call) and ((isinstance(node.value.func,ast.Attribute) and node.value.func.attr=='open') or (isinstance(node.value.func,ast.Name) and node.value.func.id=='open')):
                    for target in node.targets:
                        if isinstance(target,ast.Name):opened[target.id]=node.lineno
            for node in ast.walk(tree):
                if not (isinstance(node,ast.Call) and isinstance(node.func,ast.Attribute) and node.func.attr=='Popen'):continue
                for keyword in node.keywords:
                    if isinstance(keyword.value,ast.Name) and keyword.value.id in opened:
                        name=keyword.value.id
                        cleanup=any(isinstance(call,ast.Call) and isinstance(call.func,ast.Attribute) and call.func.attr=='addCleanup' and any(isinstance(arg,ast.Attribute) and isinstance(arg.value,ast.Name) and arg.value.id==name and arg.attr=='close' for arg in call.args) for call in ast.walk(tree))
                        self.assertTrue(cleanup,f'{path.name}:{opened[name]}: named subprocess logs require a context or registered cleanup')
    def test_native_process_lists_close_parent_pipes_after_reaping(self):
        for path in Path(__file__).parent.glob('*.py'):
            if path.name.startswith('test_'):continue
            source=path.read_text();tree=ast.parse(source)
            pipe=any(isinstance(n,ast.Attribute) and n.attr=='PIPE' for n in ast.walk(tree))
            owned_list=any(isinstance(n,ast.Assign) and isinstance(n.value,ast.List) and any(isinstance(t,ast.Name) and t.id in ('processes','procs') for t in n.targets) for n in ast.walk(tree))
            if pipe and owned_list:self.assertIn('close_reaped_pipes(',source,path.name)
    def test_actual_child_pipe_lifetime_and_idempotent_close(self):
        child=subprocess.Popen([sys.executable,'-c','import sys; value=sys.stdin.readline(); print(value.strip(),flush=True); print("error",file=sys.stderr,flush=True)'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        try:
            with self.assertRaises(RuntimeError):close_reaped_pipes([child])
            self.assertFalse(child.stdin.closed);child.stdin.write('observed\n');child.stdin.flush();child.wait(timeout=5)
            self.assertEqual(child.stdout.read(),'observed\n');self.assertEqual(child.stderr.read(),'error\n')
            close_reaped_pipes([child]);self.assertTrue(all(stream.closed for stream in (child.stdin,child.stdout,child.stderr)));close_reaped_pipes([child])
        finally:
            if child.poll() is None:child.kill();child.wait(timeout=5)
            close_reaped_pipes([child])

if __name__=='__main__':unittest.main()
