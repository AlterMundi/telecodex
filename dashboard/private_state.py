"""Small owner-local JSON records with checked, atomic writes."""
import json
import os
from pathlib import Path
import stat
import tempfile


def private_directory(path):
    path = Path(path)
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError('State directory must be owner-only and regular')
    return path


def read_json(path):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd) as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise ValueError('State record must be owner-only and regular')
        raw = stream.read(65537)
        if len(raw) > 65536:
            raise ValueError('State record exceeds bound')
        return json.loads(raw)


def write_json(path, value):
    path = Path(path)
    private_directory(path.parent)
    if path.exists() or path.is_symlink():
        read_json(path)  # Refuse unsafe destinations, including dangling symlinks.
    fd, name = tempfile.mkstemp(prefix='.write-', dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            json.dump(value, stream)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
        parent_fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(parent_fd)
        finally:
            os.close(parent_fd)
    finally:
        if os.path.exists(name):
            os.unlink(name)
