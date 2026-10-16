"""Locale and export checks for shared utility controls, using only temporary files."""
import os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import context_tools as tools

class AppearanceUtilitiesTests(unittest.TestCase):
    def test_language_is_explicit_and_rejects_unsupported_arguments(self):
        self.assertEqual(tools.language(), 'en_GB')
        for name in ('en_GB','en_US'):self.assertEqual(tools.language(name),name)
        for name in ('en;echo bad','fr_FR','',False):
            with self.assertRaises(ValueError):tools.language(name)

    def test_personal_dictionaries_do_not_cross_language_boundaries(self):
        with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{'XDG_STATE_HOME':tmp}):
            gb=tools.dictionary('en_GB');us=tools.dictionary('en_US')
            self.assertNotEqual(gb,us)
            gb.write_text('UK fixture')
            self.assertNotEqual(gb.read_text(),us.read_text())
            self.assertEqual(us.stat().st_mode & 0o777,0o600)

    def test_spelling_uses_selected_language_as_one_argument(self):
        with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{'XDG_STATE_HOME':tmp}),patch.object(tools.subprocess,'run') as run:
            run.return_value.stdout='@(#) fixture\n\n'
            self.assertEqual(tools.check_spelling('colour','en_GB'),{'spelling':[]})
            self.assertIn('--lang=en_GB',run.call_args.args[0])
            tools.check_spelling('color','en_US')
            self.assertIn('--lang=en_US',run.call_args.args[0])

    def test_subject_export_requires_explicit_overwrite_consent(self):
        from gi.repository import Gio,GLib
        with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{'XDG_CACHE_HOME':tmp}):
            folder=Path(tmp)/'lumen-desktop/subjects';folder.mkdir(parents=True)
            source=folder/('a'*32+'.png');source.write_bytes(b'new PNG fixture')
            dest=Path(tmp)/'output.png';dest.write_bytes(b'previous contents')
            with self.assertRaises(GLib.Error):tools.save_subject(str(source),str(dest))
            self.assertEqual(dest.read_bytes(),b'previous contents')
            tools.save_subject(str(source),str(dest),True)
            self.assertEqual(dest.read_bytes(),source.read_bytes())

    def test_failed_subject_export_preserves_confirmed_existing_file(self):
        import shutil
        with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{'XDG_CACHE_HOME':tmp}):
            folder=Path(tmp)/'lumen-desktop/subjects';folder.mkdir(parents=True)
            source=folder/('a'*32+'.png');source.write_bytes(b'new fixture')
            destination=Path(tmp)/'out.png';destination.write_bytes(b'existing')
            def interrupted(source,stream):
                stream.write(b'partial');raise OSError('simulated read failure')
            with patch.object(shutil,'copyfileobj',side_effect=interrupted),self.assertRaises(OSError):
                tools.save_subject(str(source),str(destination),True)
            self.assertEqual(destination.read_bytes(),b'existing')
            self.assertFalse(list(Path(tmp).glob('.lumen-export-*')))

if __name__=='__main__':unittest.main()
