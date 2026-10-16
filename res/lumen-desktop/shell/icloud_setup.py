"""Explicit local desktop/terminal iCloud setup; app password lives in Secret Service only."""
import argparse
import configparser
import contextlib
import io
from datetime import datetime, timezone
import fcntl
import getpass
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def paths():
    data = Path(os.environ.get('XDG_DATA_HOME', str(Path.home() / '.local/share')))
    root = data / 'calendar'
    return root, root / 'icloud-settings'


def private_write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, temp = tempfile.mkstemp(prefix='.' + path.name, dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp, path)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def configuration(root, settings, username, secret_tool):
    data = root.parent
    quote = json.dumps
    output = '[general]\nstatus_path = ' + quote(str(settings / 'sync-state')) + '\n'
    for kind, storage, url, suffix, target in (
        ('calendar', 'caldav', 'https://caldav.icloud.com/', '.ics', root / 'icloud'),
        ('contacts', 'carddav', 'https://contacts.icloud.com/', '.vcf', data / 'contacts/icloud'),
    ):
        output += f'''\n[pair icloud_{kind}]
a = "icloud_{kind}_local"
b = "icloud_{kind}_remote"
collections = ["from b"]
conflict_resolution = "b wins"
metadata = ["displayname"]

[storage icloud_{kind}_local]
type = "filesystem"
path = {quote(str(target))}
fileext = "{suffix}"

[storage icloud_{kind}_remote]
type = "{storage}"
url = "{url}"
username = {quote(username)}
password.fetch = {quote(['command', secret_tool, 'lookup', 'service', 'lumen-desktop-icloud', 'account', username])}
read_only = true
'''
    return output


def status(settings):
    result = dict(configured=(settings / 'vdirsyncer.conf').is_file(), lastSuccess='', error='')
    try:
        saved = json.loads((settings / 'status.json').read_text())
        result.update(lastSuccess=saved.get('lastSuccess', ''), error=saved.get('error', ''))
    except (ValueError, OSError, AttributeError):
        pass
    return result


def sync_failure(diagnostic=''):
    """Return useful fixed text; never surface raw credential/account-bearing logs."""
    text = diagnostic.decode(errors='replace') if isinstance(diagnostic, bytes) else str(diagnostic)
    if re.search(r'\b401\b|\bunauthorized\b', text, re.I):
        return ('iCloud rejected the account or password (401 Unauthorized). Use an Lumen '
                'app-specific password, not your usual Lumen Account password. Check the '
                'account email and update the saved app-specific password in Account Setup.')
    if re.search(r'\b403\b|\bforbidden\b', text, re.I):
        return 'iCloud refused access (403 Forbidden). Check account access and the app-specific password in Account Setup.'
    if 'certificate' in text.lower():
        return 'The secure connection to iCloud could not be verified. Check the system clock and network connection.'
    if 'aborted' in text.lower() or 'eoferror' in text.lower():
        return 'Calendar discovery was not completed. Open Account Setup and finish its local collection prompts.'
    return 'iCloud sync failed. Check your network and account setup, then retry.'


def validate_local_discovery(config):
    """Only auto-create local mirrors; reject custom pairs or remote-write configs."""
    settings = config.parent
    root = settings.parent
    cfg = configparser.ConfigParser(interpolation=None)
    try:
        cfg.read(config)
        if {s for s in cfg.sections() if s.startswith('pair ')} != {'pair icloud_calendar', 'pair icloud_contacts'}:
            raise ValueError()
        for kind, storage, url, target in (
            ('calendar', 'caldav', 'https://caldav.icloud.com/', root/'icloud'),
            ('contacts', 'carddav', 'https://contacts.icloud.com/', root.parent/'contacts/icloud'),
        ):
            pair = cfg['pair icloud_' + kind]
            local = cfg['storage icloud_' + kind + '_local']
            remote = cfg['storage icloud_' + kind + '_remote']
            expected = {'a':'icloud_'+kind+'_local', 'b':'icloud_'+kind+'_remote', 'collections':['from b'], 'conflict_resolution':'b wins'}
            if any(json.loads(pair[key]) != value for key, value in expected.items()):
                raise ValueError()
            if (json.loads(local['type']) != 'filesystem' or json.loads(local['path']) != str(target)
                    or json.loads(remote['type']) != storage or json.loads(remote['url']) != url
                    or json.loads(remote['read_only']) is not True):
                raise ValueError()
            if target.is_symlink():
                raise ValueError()
    except (ValueError, KeyError, configparser.Error):
        raise ValueError('Automatic discovery requires the original read-only iCloud mirror configuration. Custom storage settings need manual discovery.') from None


def discover_collections(command):
    validate_local_discovery(Path(command[2]))
    print('Preparing local calendar and contact folders. Nothing will be created in iCloud.', flush=True)
    # With both exact pairs constrained to collections=["from b"], missing
    # collections can only be created in the local filesystem storages.
    # Avoid prompting once per calendar: these folders are part of account setup.
    subprocess.run(command + ['discover'], input='y\n' * 4096, stdout=subprocess.DEVNULL,
                   stderr=subprocess.PIPE, text=True, check=True, timeout=120)


def run_sync(settings, discover=False):
    config = settings / 'vdirsyncer.conf'
    if not config.is_file():
        raise ValueError('iCloud is not configured. Run lumen-icloud-setup setup in a terminal.')
    settings.mkdir(parents=True, exist_ok=True, mode=0o700)
    lock_fd = os.open(settings / 'sync.lock', os.O_CREAT | os.O_WRONLY, 0o600)
    with os.fdopen(lock_fd, 'w') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError('An iCloud sync is already running') from None
        result = status(settings)
        command = ['vdirsyncer', '--config', str(config)]
        try:
            if discover:
                # Discovery may ask to create LOCAL mirror collections. Remote stores are read-only.
                discover_collections(command)
            subprocess.run(command + ['sync'], check=True, timeout=75, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
            subprocess.run(command + ['metasync'], check=True, timeout=15, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        except (subprocess.SubprocessError, OSError) as error:
            result['error'] = ('iCloud did not respond in time. Check the connection and retry.'
                               if isinstance(error, subprocess.TimeoutExpired)
                               else sync_failure(getattr(error, 'stderr', '')))
            private_write(settings / 'status.json', json.dumps(result))
            raise ValueError(result['error']) from None
        result['lastSuccess'] = datetime.now(timezone.utc).isoformat(timespec='seconds')
        result['error'] = ''
        private_write(settings / 'status.json', json.dumps(result))
        print('iCloud calendars and contacts downloaded. Remote data is read-only.')


def setup(root, settings):
    if not sys.stdin.isatty():
        raise ValueError('Account setup requires a local interactive terminal; never send passwords through chat.')
    config = settings / 'vdirsyncer.conf'
    if config.exists():
        print('Existing iCloud configuration retained.')
        if input('Update the saved app-specific password? [y/N]: ').strip().lower() == 'y':
            saved = configparser.ConfigParser(interpolation=None)
            saved.read(config)
            username = json.loads(saved['storage icloud_calendar_remote']['username'])
            save_password(username)
        print('Finishing discovery and syncing read-only mirrors.')
        run_sync(settings, discover=True)
        return
    secret_tool = shutil.which('secret-tool')
    if not secret_tool:
        raise ValueError('secret-tool is unavailable. Install the desktop integration before setup.')
    print('Use an app-specific password from your cloud account sign-in and security settings.')
    print('Only local read-only calendar and contact mirrors will be created. No remote events or contacts are modified.')
    username = input('Lumen Account email: ').strip()
    if not username or len(username) > 254 or '\n' in username or '\r' in username or '@' not in username:
        raise ValueError('Enter your Lumen Account email address')
    save_password(username)
    private_write(config, configuration(root, settings, username, secret_tool))
    run_sync(settings, discover=True)


def setup_gui(root, settings, request):
    """Explicit same-user UI request. Credentials never enter argv or config."""
    if not isinstance(request, dict):
        raise ValueError('Enter an account email and an app-specific password.')
    username, password = request.get('email'), request.get('password')
    if (not isinstance(username, str) or not re.fullmatch(r'[^\s@]+@[^\s@]+', username) or len(username)>254
            or not isinstance(password, str) or not password.strip() or len(password)>1024 or '\n' in password or '\r' in password):
        raise ValueError('Enter a valid account email and app-specific password.')
    config = settings/'vdirsyncer.conf'
    if config.exists():
        saved = configparser.ConfigParser(interpolation=None)
        try:
            saved.read(config)
            existing = json.loads(saved['storage icloud_calendar_remote']['username'])
        except (ValueError,KeyError,configparser.Error):
            raise ValueError('The existing account configuration needs review before changing it.') from None
        if existing != username:
            raise ValueError('Use the existing iCloud account email to update its password. Switching accounts requires a separate migration.')
        validate_local_discovery(config)
    tool = shutil.which('secret-tool')
    if not tool:
        raise ValueError('The desktop keyring is unavailable.')
    save_password(username,password.strip())
    if not config.exists():
        private_write(config, configuration(root,settings,username,tool))
    run_sync(settings,discover=True)


def save_password(username, password=None):
    secret_tool = shutil.which('secret-tool')
    if not secret_tool:
        raise ValueError('secret-tool is unavailable')
    if password is None:
        print('Copy an Lumen app-specific password from Bitwarden, then paste with Ctrl+Shift+V.')
        print('The password prompt shows no characters or dots. Paste once, then press Enter.')
        password = getpass.getpass('App-specific password (hidden): ').strip()
    if not password or '\n' in password or '\r' in password:
        raise ValueError('An app-specific password is required')
    # Password is stdin, never a command argument, environment value or saved config field.
    try:
        subprocess.run([secret_tool, 'store', '--label=Lumen Desktop iCloud', 'service', 'lumen-desktop-icloud', 'account', username], input=password, text=True, check=True, timeout=60, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print('Password received and saved to the desktop keyring. iCloud sign-in still needs to be verified.')
    except (OSError, subprocess.SubprocessError):
        raise ValueError('Could not save the app password in the unlocked desktop keyring') from None
    finally:
        password = None

def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['setup', 'setup-json', 'sync', 'status'], nargs='?', default='setup')
    args = parser.parse_args()
    root, settings = paths()
    try:
        if args.action == 'setup-json':
            try:
                raw = sys.stdin.readline(8193)
                if len(raw)>8192:
                    raise ValueError()
                request = json.loads(raw)
            except (ValueError,TypeError):
                raise ValueError('Account setup request could not be read.') from None
            # Human-oriented sync output is never mixed with the UI protocol.
            with contextlib.redirect_stdout(io.StringIO()):
                setup_gui(root,settings,request)
            print(json.dumps({'success':True}))
        elif args.action == 'status':
            print(json.dumps(status(settings)))
        elif args.action == 'setup':
            setup(root, settings)
        else:
            run_sync(settings)
    except (ValueError, OSError, EOFError, KeyboardInterrupt) as exc:
        if args.action == 'setup-json':
            print(json.dumps({'error':str(exc) or 'Setup cancelled.'}))
        else:
            print(str(exc) or 'Setup cancelled.', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
