"""Exact identity rows also cover personal Trash, without deleting any file."""
import json,os,subprocess,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'res/lumen-desktop/shell'))
import file_identity,file_jobs

class FilesSelection(unittest.TestCase):
 def test_personal_trash_rows_publish_canonical_opened_identity(self):
  with tempfile.TemporaryDirectory(prefix='lumen-selection-trash-') as directory,patch.dict(os.environ,XDG_DATA_HOME=directory):
   root=Path(directory)/'Trash';(root/'files').mkdir(parents=True);(root/'info').mkdir()
   item=root/'files/entry';item.write_text('synthetic recoverable contents')
   (root/'info/entry.trashinfo').write_text('[Trash Info]\nPath=/synthetic/original.txt\nDeletionDate=2026-01-01T00:00:00\n')
   rows=file_jobs.trash_items();self.assertEqual(len(rows),1);self.assertEqual(rows[0]['identity'],file_identity.wire(item.lstat()));self.assertEqual(item.read_text(),'synthetic recoverable contents')
 def test_selection_models(self):
  subprocess.run([os.environ.get('NODE_BINARY','node'),str(ROOT/'tests/desktop/test_files_selection_model.cjs')],cwd=ROOT,check=True)

if __name__=='__main__':unittest.main()
