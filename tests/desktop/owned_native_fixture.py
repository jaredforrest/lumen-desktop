"""Reproducible namespace entry point for owned native presentation fixtures."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from application_fixture.isolation import run
from application_fixture.offscreen import immutable_file

FIXTURES = ('qt_pointer_retirement_input', 'image_markup_input', 'owned_preview_input')


def snapshot(source, fixture):
    def git(*args):
        return subprocess.check_output(['git', '-C', str(source), *args], text=True).strip()
    if git('status', '--porcelain'):
        raise ValueError('Freeze source before native acceptance')
    paths = [Path(__file__).resolve(), source / 'tests/desktop' / (fixture + '.py'),
             source / 'tests/desktop/application_fixture/isolation.py']
    return {'revision': git('rev-parse', 'HEAD'),
            'hashes': {str(path.relative_to(source)): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}}


def command(fixture, launcher, sway, pointer=None, keyboard=None, at_spi=None, effects=False):
    if fixture not in FIXTURES:
        raise ValueError('Unsupported owned native fixture')
    args = ['--launcher', str(immutable_file(launcher)), '--sway', str(immutable_file(sway)),
            '--output', '/artifacts/result']
    if fixture != 'owned_preview_input':
        args += ['--pointer-tool', str(immutable_file(pointer))]
    if fixture == 'image_markup_input':
        args += ['--keyboard', str(immutable_file(keyboard)), '--at-spi-prefix', str(at_spi)]
        if not Path(at_spi).resolve().is_relative_to('/nix/store') or not Path(at_spi).is_dir():
            raise ValueError('Pinned accessibility runtime required')
    if effects:
        args += ['--effects']
    return args


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('fixture', choices=FIXTURES)
    parser.add_argument('--launcher', required=True)
    parser.add_argument('--sway', required=True)
    parser.add_argument('--pointer-tool')
    parser.add_argument('--keyboard')
    parser.add_argument('--at-spi-prefix', default=os.environ.get('LUMEN_TEST_AT_SPI'))
    parser.add_argument('--effects', action='store_true')
    parser.add_argument('--output', required=True, type=Path)
    a = parser.parse_args()
    source = Path(__file__).resolve().parents[2]
    before = snapshot(source, a.fixture)
    python = immutable_file(sys.executable)
    args = command(a.fixture, a.launcher, a.sway, a.pointer_tool, a.keyboard, a.at_spi_prefix, a.effects)
    tools = [python, immutable_file(a.launcher), immutable_file(a.sway)]
    if a.fixture != 'owned_preview_input': tools.append(immutable_file(a.pointer_tool))
    if a.fixture == 'image_markup_input': tools.append(immutable_file(a.keyboard))
    tool_hashes = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in tools}
    environment = {}
    if a.effects:
        environment['__EGL_VENDOR_LIBRARY_FILENAMES'] = str(immutable_file(os.environ['__EGL_VENDOR_LIBRARY_FILENAMES']))
    output = a.output.resolve()
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    # Only validated immutable runtime metadata crosses the namespace boundary.
    code = ('import os,runpy,sys;os.environ.update(' + repr(environment) + ');'
            'sys.path.insert(0,"/source/tests/desktop");sys.argv=' + repr([a.fixture + '.py'] + args) + ';'
            'runpy.run_path("/source/tests/desktop/' + a.fixture + '.py",run_name="__main__")')
    # A managed worktree's .git points outside its checkout. Mount a real,
    # self-contained repository so inner provenance uses genuine Git objects.
    with tempfile.TemporaryDirectory(prefix='lumen-owned-native-source-') as private:
        capsule = Path(private) / 'source'
        subprocess.run(['git', 'clone', '--quiet', '--no-local', '--no-hardlinks', '--no-checkout',
                        str(source), str(capsule)], check=True)
        subprocess.run(['git', '-C', str(capsule), 'checkout', '--quiet', '--detach', before['revision']], check=True)
        if not (capsule / '.git').is_dir() or (capsule / '.git/objects/info/alternates').exists():
            raise ValueError('A self-contained source repository is required')
        status = run(capsule, output, [str(python), '-c', code], software_graphics=a.effects)
    try:
        unchanged = snapshot(source, a.fixture) == before and all(
            hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest for path, digest in tool_hashes.items())
    except (ValueError, OSError, subprocess.SubprocessError):
        unchanged = False
    record = dict(source=before, fixture=a.fixture, sourceUnchanged=unchanged,
                  toolHashes=tool_hashes, accessibilityRuntime=a.at_spi_prefix if a.fixture == 'image_markup_input' else None,
                  launcher=str(Path(a.launcher).resolve()), compositor=str(Path(a.sway).resolve()),
                  command=[str(python), '-c', code], softwareGraphics=a.effects, exitCode=status)
    (output / 'namespace-provenance.json').write_text(json.dumps(record, indent=2) + '\n')
    if not unchanged:
        raise ValueError('Native acceptance source changed during execution')
    return status


if __name__ == '__main__':
    raise SystemExit(main())
