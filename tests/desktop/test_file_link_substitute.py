"""Real disposable symbolic-link identity, moved-target and recovery contracts."""
import errno
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import files
import file_link_state


class LinkSubstitute(unittest.TestCase):
    def test_cyclic_link_remains_listed_and_no_recursive_traversal_occurs(self):
        with tempfile.TemporaryDirectory() as directory:
            link=Path(directory)/'cycle';link.symlink_to(link.name)
            result=files.listing({'path':directory})
            self.assertEqual(len(result['rows']),1)
            row=result['rows'][0]
            self.assertTrue(row['link']);self.assertFalse(row['directory'])
            self.assertEqual(row['linkStatus'],'chain-limit')
            self.assertEqual(result['unreadable'],0)
            with patch.object(files.subprocess,'run') as dispatch:
                with self.assertRaisesRegex(OSError,'cyclic or exceeds'):
                    files.handle({'action':'open','path':str(link)})
                dispatch.assert_not_called()

    def test_moved_target_has_recoverable_error_and_never_dispatches(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);target=root/'literal ü $(ignored).txt';target.write_bytes(b'original')
            result=files.handle({'action':'make-link','path':str(target)})
            link=Path(result['selected']);moved=root/'moved.txt';target.rename(moved)
            with patch.object(files.subprocess,'run') as dispatch:
                with self.assertRaisesRegex(OSError,'File Info'):
                    files.handle({'action':'open','path':str(link)})
                dispatch.assert_not_called()
            row=next(r for r in files.listing({'path':directory})['rows'] if r['path']==str(link))
            self.assertEqual(row['linkStatus'],'broken');self.assertTrue(row['linkError'])
            info=files.handle({'action':'info','path':str(link)})['info']
            self.assertEqual(info['kind'],'Symbolic link');self.assertEqual(info['target'],str(target))
            self.assertEqual(info['linkStatus'],'broken');self.assertEqual(moved.read_bytes(),b'original')

    def test_creation_collision_and_literal_dispatch_do_not_modify_target(self):
        with tempfile.TemporaryDirectory() as directory:
            target=Path(directory)/'literal # ü $(ignored).txt';target.write_bytes(b'original')
            first=Path(files.handle({'action':'make-link','path':str(target)})['selected'])
            second=Path(files.handle({'action':'make-link','path':str(target)})['selected'])
            self.assertNotEqual(first,second);self.assertEqual(os.readlink(first),str(target))
            with patch.object(files.subprocess,'run') as dispatch,patch.dict(os.environ,{'LUMEN_LAUNCH_SCOPES':'0'}):
                self.assertTrue(files.handle({'action':'open','path':str(first)})['ok'])
                self.assertEqual(dispatch.call_args.args[0],['xdg-open',first.as_uri()])
            first.unlink();second.unlink();self.assertEqual(target.read_bytes(),b'original')

    def test_directory_link_is_explicitly_a_link_while_navigation_can_follow(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);target=root/'folder';target.mkdir();(target/'kept').write_text('unchanged')
            link=root/'shortcut';link.symlink_to(target,target_is_directory=True)
            row=next(r for r in files.listing({'path':directory})['rows'] if r['path']==str(link))
            self.assertTrue(row['directory'] and row['link']);self.assertEqual(row['linkStatus'],'resolved')
            self.assertEqual([r['name'] for r in files.listing({'path':str(link)})['rows']],['kept'])
            self.assertEqual(files.handle({'action':'info','path':str(link)})['info']['kind'],'Symbolic link')

    def test_permission_and_unknown_target_errors_keep_link_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);target=root/'target';target.write_text('kept');link=root/'link';link.symlink_to(target)
            for code,expected in [(errno.EACCES,'permission-denied'),(errno.EPERM,'permission-denied'),(errno.EIO,'unavailable')]:
                with self.subTest(code=code),patch.object(file_link_state,'target_info',side_effect=OSError(code,'fixture')):
                    row=next(r for r in files.listing({'path':directory})['rows'] if r['path']==str(link))
                    self.assertEqual(row['linkStatus'],expected);self.assertEqual(row['identity']['inode'],str(link.lstat().st_ino))
                    with patch.object(files.subprocess,'run') as dispatch:
                        with self.assertRaisesRegex(OSError,'File Info'):files.handle({'action':'open','path':str(link)})
                        dispatch.assert_not_called()
            self.assertEqual(target.read_text(),'kept')

    def test_regular_files_do_not_probe_a_link_target(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'file';path.write_text('kept')
            with patch.object(file_link_state,'target_info',side_effect=AssertionError('Unexpected target probe')):
                row=files.listing({'path':directory})['rows'][0]
                self.assertFalse(row['link']);self.assertEqual(row['linkStatus'],'')


if __name__=='__main__':unittest.main()
