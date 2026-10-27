"""Opaque immutable original attachments; retention never admits model work."""
import hashlib
import os
from pathlib import Path
import re
import stat
import tempfile
import uuid
from .private_state import private_directory
from scripts.speech_render import sync_directory

MAX_BYTES=20*1024*1024
MAX_FILES=8

def check_reference(files):
    if files is None:return
    if not isinstance(files,list) or len(files)>MAX_FILES:raise ValueError('Select at most eight files')
    ids=set();total=0
    for ref in files:
        if not isinstance(ref,dict) or set(ref)!={'file_id','sha256','name','mime','size'}:raise ValueError('Select original files again')
        ident=ref['file_id'];name=ref['name'];mime=ref['mime'];size=ref['size']
        if (not isinstance(ident,str) or str(uuid.UUID(ident))!=ident or ident in ids
                or not isinstance(ref['sha256'],str) or not re.fullmatch('[a-f0-9]{64}',ref['sha256'])
                or not isinstance(name,str) or not name or len(name.encode())>240 or name in ('.','..')
                or any(ord(c)<32 or ord(c)==127 or c in '/\\' for c in name)
                or not isinstance(mime,str) or len(mime)>128 or not re.fullmatch(r'[A-Za-z0-9!#$&^_.+-]+/[A-Za-z0-9!#$&^_.+-]+',mime)
                or type(size) is not int or not 0<=size<=MAX_BYTES):raise ValueError('Invalid bounded original file')
        ids.add(ident);total+=size
    if total>2*MAX_BYTES:raise ValueError('Select files totaling at most forty MiB')

def read_file(db,ref):
    check_reference([ref])
    directory=Path(str(db)+'.files');m=directory.lstat()
    if not stat.S_ISDIR(m.st_mode) or m.st_uid!=os.getuid() or m.st_mode&0o077:raise ValueError('Private file storage required')
    fd=os.open(directory/ref['sha256'],os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK)
    with os.fdopen(fd,'rb') as stream:
        m=os.fstat(stream.fileno())
        if not stat.S_ISREG(m.st_mode) or m.st_uid!=os.getuid() or m.st_mode&0o077 or m.st_size!=ref['size']:raise ValueError('Original file changed')
        data=stream.read(MAX_BYTES+1)
    if len(data)!=ref['size'] or hashlib.sha256(data).hexdigest()!=ref['sha256']:raise ValueError('Original file checksum changed')
    return data

def retain_file(db,data,ref):
    check_reference([ref])
    if not isinstance(data,bytes) or len(data)!=ref['size'] or hashlib.sha256(data).hexdigest()!=ref['sha256']:raise ValueError('Original upload checksum changed')
    directory=private_directory(Path(str(db)+'.files'));sync_directory(directory.parent)
    fd,name=tempfile.mkstemp(prefix='.file-',dir=directory)
    try:
        with os.fdopen(fd,'wb') as stream:stream.write(data);stream.flush();os.fsync(stream.fileno())
        try:os.link(name,directory/ref['sha256'],follow_symlinks=False)
        except FileExistsError:read_file(db,ref)
        sync_directory(directory)
    finally:os.unlink(name)
    return {**ref,'media':'retained','work_admitted':False}
