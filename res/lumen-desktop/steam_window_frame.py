"""Remove Steam's duplicate caption controls from its generated UI styles."""
from pathlib import Path
import subprocess
import sys

MARKER = '/* Nix native window controls */'
RULE = '''
/* Nix native window controls */
.title-bar-actions .minimizeButton,
.title-bar-actions .maximizeButton,
.title-bar-actions .restoreButton,
.title-bar-actions .closeButton { display: none !important; }
'''


def update(file):
    if file.suffix != '.css' or not file.is_file():
        return
    text = file.read_text()
    if MARKER in text or '.title-bar-actions' not in text:
        return
    # Append a style override; never rewrite Steam's configuration or account data.
    with file.open('a') as stream:
        stream.write(RULE)


def main():
    root = Path(sys.argv[1])
    for file in root.glob('*.css'):
        update(file)
    # Steam updates replace these cache files. Reapply the same Nix-owned rule.
    with subprocess.Popen(['inotifywait', '-m', '-q', '-e', 'close_write,moved_to',
                           '--format', '%f', str(root)], stdout=subprocess.PIPE, text=True) as watcher:
        for name in watcher.stdout:
            update(root / Path(name.strip()).name)
        raise SystemExit(watcher.wait())


if __name__ == '__main__':
    main()
