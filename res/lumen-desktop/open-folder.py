"""Adapt desktop-entry file URLs to the shell's local-directory IPC.

This is a folder handler, not a generic opener. Reject unsupported URLs/files
instead of invoking xdg-open recursively or silently navigating somewhere else.
"""

import argparse
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote, urlsplit


def directory_path(value):
    if not value or "\0" in value:
        raise ValueError("empty or invalid folder argument")
    if value.lower().startswith("file:"):
        uri = urlsplit(value)
        if uri.netloc.lower() not in ("", "localhost"):
            raise ValueError("remote file URLs are not supported")
        if uri.query or uri.fragment:
            raise ValueError("file URLs must escape query and fragment characters")
        if re.search(r"%(?![0-9a-fA-F]{2})", uri.path):
            raise ValueError("invalid percent escape in file URL")
        path = unquote(uri.path, errors="strict")
        if not os.path.isabs(path):
            raise ValueError("file URL must contain an absolute path")
    else:
        # Accept literal relative paths (including colons) if they really exist;
        # an unsupported URL must never become a made-up local directory.
        path = value
        if not Path(path).is_dir() and urlsplit(value).scheme:
            raise ValueError("only local folders and file URLs are supported")
    if "\0" in path or not Path(path).is_dir():
        raise ValueError("folder does not exist or is not a directory")
    return os.path.abspath(path)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quickshell", required=True)
    parser.add_argument("--shell", required=True)
    parser.add_argument("folders", nargs="*")
    args = parser.parse_args(argv)
    try:
        # Validate all inputs first; a bad second URL must not partially launch.
        paths = [directory_path(value) for value in args.folders] or [""]
        for index, path in enumerate(paths):
            result = subprocess.run(
                [args.quickshell, "-p", args.shell, "ipc", "call", "desktop",
                 "files" if index == 0 else "filesNew", path],
                timeout=10,
            )
            if result.returncode:
                return result.returncode if result.returncode > 0 else 1
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        print(f"Files could not open folder: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
