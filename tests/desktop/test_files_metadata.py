"""Regression: recursive Files copies retain directory privacy and timestamps."""
import importlib.util
import os
import stat
import tempfile
from pathlib import Path

SOURCE = Path(__file__).resolve().parents[2] / 'res/lumen-desktop/shell/files.py'
spec = importlib.util.spec_from_file_location('files', SOURCE)
files = importlib.util.module_from_spec(spec)
spec.loader.exec_module(files)

with tempfile.TemporaryDirectory(prefix='files-metadata-') as temp:
    root = Path(temp)
    try:
        for root_mode in (0o700, 0o555):
            source = root / f'source-{root_mode:o}'
            destination = root / f'destination-{root_mode:o}'
            source.mkdir()
            destination.mkdir()
            (source / 'readonly').mkdir()
            (source / 'public').mkdir()
            (source / 'readonly/private.txt').write_text('private payload')
            (source / 'public/public.txt').write_text('public payload')
            outside = root / f'outside-{root_mode:o}'
            outside.mkdir()
            (outside / 'keep').write_text('external data')
            (source / 'linked').symlink_to(outside, target_is_directory=True)
            (source / 'broken').symlink_to(source / 'absent')
            modes = {source: root_mode, source / 'readonly': 0o500, source / 'public': 0o755}
            for index, (directory, mode) in enumerate(modes.items()):
                directory.chmod(mode)
                os.utime(directory, ns=(1512345678123456789 + index, 1612345678987654321 + index))
            expected = {p.relative_to(source): (stat.S_IMODE(p.stat().st_mode), p.stat().st_atime_ns, p.stat().st_mtime_ns) for p in modes}
            copied = destination / source.name
            original_copy = files.copy_item
            intermediate_modes = []

            def observe_copy(src, dst):
                if dst != copied:
                    intermediate_modes.append(stat.S_IMODE(dst.parent.stat().st_mode))
                    assert intermediate_modes[-1] == 0o700, (dst, intermediate_modes[-1])
                return original_copy(src, dst)

            files.copy_item = observe_copy
            previous_umask = os.umask(0)
            try:
                result = files.handle({'action': 'copy', 'path': str(source), 'destination': str(destination)})
            finally:
                os.umask(previous_umask)
                files.copy_item = original_copy
            assert result['ok'] and intermediate_modes
            # Check timestamps before reading destination directories.
            for relative, metadata in expected.items():
                info = (copied / relative).stat()
                assert (stat.S_IMODE(info.st_mode), info.st_atime_ns, info.st_mtime_ns) == metadata, (relative, metadata, info)
            assert (copied / 'readonly/private.txt').read_text() == 'private payload'
            assert (copied / 'linked').is_symlink()
            assert os.readlink(copied / 'linked') == str(outside)
            assert (copied / 'broken').is_symlink() and not (copied / 'broken').exists()
            try:
                files.handle({'action': 'copy', 'path': str(source), 'destination': str(destination)})
                raise AssertionError('Existing directory must not be overwritten')
            except FileExistsError:
                pass
            assert (copied / 'readonly/private.txt').read_text() == 'private payload'
            assert (outside / 'keep').read_text() == 'external data'

        source = root / 'failure-source'
        source.mkdir()
        (source / 'fail.txt').write_text('copy failure fixture')
        destination = root / 'failure-destination'
        destination.mkdir()
        original_file = files.file
        files.file = lambda _: (_ for _ in ()).throw(OSError('simulated read failure'))
        try:
            try:
                files.handle({'action': 'copy', 'path': str(source), 'destination': str(destination)})
                raise AssertionError('Expected the injected copy failure')
            except RuntimeError:
                pass
        finally:
            files.file = original_file
        assert stat.S_IMODE((destination / source.name).stat().st_mode) == 0o700
    finally:
        for directory, dirs, files in os.walk(root, topdown=True, followlinks=False):
            Path(directory).chmod(0o700)

print('PASS private/read-only/nested directory modes and nanosecond timestamps; restrictive in-progress and failed copies; symlinks and no-overwrite unchanged')
