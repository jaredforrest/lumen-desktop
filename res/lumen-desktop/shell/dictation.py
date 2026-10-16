"""On-demand microphone capture and local English transcription; never inserts text."""
import json
import os
from pathlib import Path
import subprocess
import tempfile


def recording():
    return Path(os.environ['XDG_RUNTIME_DIR'])/'lumen-dictation.wav'


def stop():
    subprocess.run(['systemctl','--user','stop','lumen-dictation.service'],capture_output=True,timeout=8,check=False)


def transcribe(path):
    model=os.environ.get('LUMEN_DICTATION_MODEL','')
    if not Path(model).is_file():raise ValueError('The local speech model is unavailable')
    with tempfile.TemporaryDirectory(prefix='lumen-transcript-',dir=os.environ['XDG_RUNTIME_DIR']) as directory:
        output=Path(directory)/'transcript'
        subprocess.run(['whisper-cli','-m',model,'-f',str(path),'-l','en','-t','4','-otxt','-of',str(output),'-nt','-np'],
            capture_output=True,check=True,timeout=90)
        return dict(text=output.with_suffix('.txt').read_text().strip(),message='Transcribed locally · review before inserting')


def handle(data):
    action=data.get('action')
    if action=='dictation-start':
        stop();recording().unlink(missing_ok=True)
        subprocess.run(['systemd-run','--user','--collect','--quiet','--unit=lumen-dictation',
            '--property=KillSignal=SIGINT','--property=RuntimeMaxSec=120','--property=UMask=0077',
            '--setenv=PATH='+os.environ['PATH'],'--setenv=XDG_RUNTIME_DIR='+os.environ['XDG_RUNTIME_DIR'],
            'pw-record','--rate=16000','--channels=1','--format=s16',str(recording())],check=True,capture_output=True,timeout=8)
        return dict(recording=True,message='Recording · microphone active · maximum 2 minutes')
    if action in ('dictation-cancel','dictation-finish'):
        stop()
        try:
            if action=='dictation-cancel':return dict(recording=False,message='Recording discarded')
            if not recording().exists() or recording().stat().st_size<1000:raise ValueError('No microphone audio was captured')
            return dict(transcribe(recording()),recording=False)
        finally:recording().unlink(missing_ok=True)
    raise ValueError('Unknown dictation action')
