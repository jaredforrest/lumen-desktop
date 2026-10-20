"""Read-only state from the fixture's explicit browser profile, never the live profile."""
from contextlib import closing
from pathlib import Path
import os,sqlite3
# Persistent DownloadState values: components/history/core/browser/download_constants.h.
IN_PROGRESS=0
COMPLETE=1
CANCELLED=2
INTERRUPTED=4

def download_rows(profile):
 path=Path(profile)/'Default/History'
 if not path.exists():return []
 with closing(sqlite3.connect(path.as_uri()+'?mode=ro',uri=True,timeout=.1)) as connection:
  connection.row_factory=sqlite3.Row
  try:
   rows=[dict(row) for row in connection.execute('SELECT id,guid,current_path,target_path,received_bytes,total_bytes,state,interrupt_reason FROM downloads ORDER BY id')]
   for row in rows:
    for key in ('current_path','target_path'):row[key]=os.fsdecode(row[key])
   return rows
  except sqlite3.OperationalError as error:
   if 'locked' in str(error):return []
   raise

def history_urls(profile):
 path=Path(profile)/'Default/History'
 if not path.exists():return []
 with closing(sqlite3.connect(path.as_uri()+'?mode=ro',uri=True,timeout=.1)) as connection:
  try:return [str(row[0]) for row in connection.execute('SELECT url FROM urls ORDER BY id DESC LIMIT 4096')]
  except sqlite3.OperationalError as error:
   if 'locked' in str(error):return []
   raise
