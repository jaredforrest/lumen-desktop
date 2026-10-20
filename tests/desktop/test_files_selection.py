"""Exact identity rows also cover personal Trash, without deleting any file."""
import json,os,subprocess,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'res/lumen-desktop/shell'))
import file_identity,file_jobs,files

class FilesSelection(unittest.TestCase):
 def test_personal_trash_rows_publish_canonical_opened_identity(self):
  with tempfile.TemporaryDirectory(prefix='lumen-selection-trash-') as directory,patch.dict(os.environ,XDG_DATA_HOME=directory):
   root=Path(directory)/'Trash';(root/'files').mkdir(parents=True);(root/'info').mkdir()
   item=root/'files/entry';item.write_text('synthetic recoverable contents')
   (root/'info/entry.trashinfo').write_text('[Trash Info]\nPath=/synthetic/original.txt\nDeletionDate=2026-01-01T00:00:00\n')
   rows=file_jobs.trash_items();self.assertEqual(len(rows),1);self.assertEqual(rows[0]['identity'],file_identity.wire(item.lstat()));self.assertEqual(item.read_text(),'synthetic recoverable contents')
 def reconcile(self,saved,rows):
  code="""const fs=require('fs'),vm=require('vm'),input=JSON.parse(fs.readFileSync(0,'utf8'));
const context={Identity:require('./res/lumen-desktop/shell/FileIdentity.js'),module:{exports:{}}};
vm.runInNewContext(fs.readFileSync('res/lumen-desktop/shell/FilesHistoryModel.js','utf8').replace(/^\\.import.*$/m,''),context);
process.stdout.write(JSON.stringify(context.module.exports.reconcile(context.module.exports.presentation(input.saved),input.rows)));"""
  return json.loads(subprocess.check_output([os.environ.get('NODE_BINARY','node'),'-e',code],cwd=ROOT,input=json.dumps(dict(saved=saved,rows=rows)),text=True))
 def test_removed_hardlink_name_never_selects_its_remaining_sibling(self):
  with tempfile.TemporaryDirectory(prefix='lumen-hardlink-selection-') as directory:
   root=Path(directory);first=root/'selected';first.write_text('shared');second=root/'remaining';os.link(first,second)
   before=files.listing(dict(path=directory))['rows'];selected=next(row for row in before if row['path']==str(first))
   self.assertFalse(selected['renameFollow']);first.unlink();after=files.listing(dict(path=directory))['rows'];self.assertTrue(after[0]['renameFollow'])
   value=self.reconcile(dict(selection=[selected],current=selected,anchor=selected),after)
   self.assertEqual(value,dict(paths=[],selected=-1,anchor=-1));self.assertEqual(second.read_text(),'shared')
 def test_single_link_rename_is_followed_but_unknown_or_directory_provenance_is_not(self):
  with tempfile.TemporaryDirectory(prefix='lumen-rename-selection-') as directory:
   root=Path(directory);first=root/'selected';first.write_text('single');before=files.listing(dict(path=directory))['rows'][0];self.assertTrue(before['renameFollow'])
   renamed=root/'renamed';first.rename(renamed);after=files.listing(dict(path=directory))['rows'];self.assertEqual(self.reconcile(dict(selection=[before],current=before),after)['paths'],[str(renamed)])
   unknown=dict(before);unknown.pop('renameFollow');self.assertEqual(self.reconcile(dict(selection=[unknown]),after)['paths'],[])
   folder=root/'folder';folder.mkdir();entry=next(row for row in files.listing(dict(path=directory))['rows'] if row['path']==str(folder));self.assertFalse(entry['renameFollow'])
   self.assertEqual(self.reconcile(dict(selection=[entry]),[entry])['paths'],[str(folder)])
 def test_selection_models(self):
  subprocess.run([os.environ.get('NODE_BINARY','node'),str(ROOT/'tests/desktop/test_files_selection_model.cjs')],cwd=ROOT,check=True)

if __name__=='__main__':unittest.main()
