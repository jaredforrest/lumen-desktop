"""Bounded read-only status and explicit opening of a consumer-supplied provider."""
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import time

MAX_BYTES = 16384
STATUS_SECONDS = 2


def normalize(value):
    if not isinstance(value, dict) or type(value.get('available')) is not bool or type(value.get('active')) is not bool:
        raise ValueError('Invalid provider status')
    detail = value.get('detail', '')
    if not isinstance(detail, str) or len(detail) > 160 or any(ord(char) < 32 for char in detail):
        raise ValueError('Invalid provider detail')
    if value['active'] and not value['available']:
        raise ValueError('Unavailable provider cannot report active')
    can_send = value.get('canSend', False)
    if type(can_send) is not bool or (can_send and not value['available']):
        raise ValueError('Invalid provider send capability')
    return dict(available=value['available'], active=value['active'], canSend=can_send, detail=detail)


def status(command, seconds=STATUS_SECONDS):
    deadline = time.monotonic() + seconds
    child = subprocess.Popen([str(command), 'status'], stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    try:
        chunks = bytearray()
        total = 0
        diagnostic = False
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ, 'output')
            selector.register(child.stderr, selectors.EVENT_READ, 'diagnostic')
            while selector.get_map():
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError('Provider status deadline')
                events = selector.select(remaining)
                if not events:
                    raise TimeoutError('Provider status deadline')
                for key, _ in events:
                    chunk = os.read(key.fd, min(4096, MAX_BYTES + 1 - total))
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    total += len(chunk)
                    if total > MAX_BYTES:
                        raise ValueError('Provider status exceeds byte budget')
                    if key.data == 'output':
                        chunks.extend(chunk)
                    else:
                        diagnostic = True
        # Leave the leader unreaped until its owned group is retired. Reaping
        # first would permit PID/group identity reuse before final cleanup.
        exited = None
        while exited is None:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError('Provider status deadline')
            exited = os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
            if exited is None:
                time.sleep(min(.005, remaining))
        if exited.si_code != os.CLD_EXITED or exited.si_status != 0:
            raise ValueError('Provider status failed')
        if diagnostic:
            raise ValueError('Provider emitted a diagnostic')
        result = normalize(json.loads(chunks))
        if time.monotonic() > deadline:
            raise TimeoutError('Provider status deadline')
        return result
    finally:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait(timeout=1)
        child.stdout.close()
        child.stderr.close()


def configured(environment):
    value = environment.get('LUMEN_NEARBY_SHARING_PROVIDER', '')
    if not value:
        return None
    path = Path(value)
    if not path.is_absolute() or not str(path.resolve()).startswith('/nix/store/') or not path.is_file() or not os.access(path, os.X_OK):
        raise ValueError('Immutable configured provider required')
    return path


def main():
    args = sys.argv[1:]
    sending = args[:1] == ['send']
    if (sending and not (1 <= len(args[1:]) <= 128 and all(Path(path).is_absolute() and '\0' not in path for path in args[1:]))) or (not sending and args not in (['status'], ['open'])):
        raise SystemExit('Expected status, open, or send with absolute local paths')
    try:
        provider = configured(os.environ)
        if sending or args == ['open']:
            if provider is None:
                raise ValueError('No nearby sharing provider configured')
            if sending and not status(provider)['canSend']:
                raise ValueError('Provider does not currently offer sending')
            os.execv(str(provider), [str(provider), *args])
        result = status(provider) if provider is not None else dict(available=False, active=False, canSend=False, detail='Not configured')
    except (OSError, ValueError, TimeoutError, subprocess.SubprocessError):
        if sending or args == ['open']:
            raise SystemExit('Nearby sharing provider is unavailable')
        result = dict(available=False, active=False, canSend=False, detail='Unavailable')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
