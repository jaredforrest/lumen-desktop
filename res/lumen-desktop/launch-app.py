"""Launch applications outside the desktop shell's service cgroup."""
import os
import sys
import uuid


def main():
    command = sys.argv[1:]
    if command[:1] == ['--']:
        command = command[1:]
    if not command:
        print('An application command is required', file=sys.stderr)
        return 2
    if os.environ.get('LUMEN_LAUNCH_SCOPES') == '1':
        # Scope mode inherits our environment and working directory. Register
        # the scope before starting the app, so all descendants inherit it.
        # Disable systemd's variable expansion to keep argv literal.
        command = [
            'systemd-run', '--user', '--scope', '--collect', '--quiet',
            '--expand-environment=no',
            '--unit=app-lumen-desktop-' + uuid.uuid4().hex,
            '--', *command,
        ]
    # Never fall back to the shell service after a failed production scope
    # launch: that would make the application vulnerable to shell restarts.
    os.execvp(command[0], command)


if __name__ == '__main__':
    try:
        sys.exit(main())
    except OSError as exc:
        print('Unable to launch application: ' + str(exc), file=sys.stderr)
        sys.exit(1)
