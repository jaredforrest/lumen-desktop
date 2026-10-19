"""Route actions to the shell service, independent of its immutable asset path."""
import argparse
import os
from pathlib import Path
import subprocess
import sys


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--systemctl', default='systemctl')
    parser.add_argument('--quickshell', default='quickshell')
    parser.add_argument('arguments', nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    arguments = args.arguments
    if arguments[:1] == ['--']:
        arguments = arguments[1:]
    if not arguments:
        parser.error('an IPC command is required')
    try:
        result = subprocess.run([args.systemctl, '--user', 'show', 'lumen-desktop-shell.service',
                                 '--property=MainPID', '--value'], check=True, capture_output=True,
                                text=True, timeout=5)
        value = result.stdout.strip()
        if not value.isascii() or not value.isdecimal() or int(value) < 2:
            raise ValueError('Desktop shell service is not running')
        pid = int(value)
        if Path(f'/proc/{pid}').stat().st_uid != os.getuid():
            raise ValueError('Desktop shell service has an unexpected owner')
        # Quickshell resolves the live instance by this exact PID. Do not retry a
        # side effect against a replacement instance if it exits during routing.
        return subprocess.run([args.quickshell, 'ipc', '--pid', str(pid), *arguments],
                              timeout=30).returncode
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        print(f'Desktop action unavailable: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
