"""Bounded, immutable PNG evidence. Uploading media admits no conversation work."""
import hashlib
import json
import os
from pathlib import Path
import stat
import struct
import subprocess
import tempfile
import zlib

from .private_state import private_directory
from scripts.speech_render import sync_directory

MAX_BYTES = 2*1024*1024


def png_dimensions(data):
    if not isinstance(data, bytes) or not 33 <= len(data) <= MAX_BYTES or data[:16] != b'\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR':
        raise ValueError('Select one bounded PNG image')
    width, height = struct.unpack('>II',data[16:24])
    if not 0 < width <= 4096 or not 0 < height <= 4096 or width*height > 4194304:
        raise ValueError('Image exceeds pixel bounds')
    offset=8
    while offset < len(data):
        if offset+12 > len(data):raise ValueError('Incomplete image')
        size=struct.unpack('>I',data[offset:offset+4])[0]
        end=offset+size+12
        if end>len(data) or zlib.crc32(data[offset+4:end-4])!=struct.unpack('>I',data[end-4:end])[0]:
            raise ValueError('Image bytes are incomplete')
        kind=data[offset+4:offset+8]
        if kind in (b'acTL',b'fcTL',b'fdAT'):raise ValueError('Select one still frame')
        offset=end
        if kind==b'IEND':
            if size or offset!=len(data):raise ValueError('Unexpected image content')
            break
    else:raise ValueError('Image is incomplete')
    return width,height


def read_png(path, expected):
    fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK)
    with os.fdopen(fd,'rb') as stream:
        info=os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_mode&0o077:
            raise ValueError('Image requires private owner storage')
        data=stream.read(MAX_BYTES+1)
    if hashlib.sha256(data).hexdigest()!=expected:raise ValueError('Selected image changed')
    png_dimensions(data)
    return data


def retain_png(db, data, expected):
    width,height=png_dimensions(data)
    if hashlib.sha256(data).hexdigest()!=expected:raise ValueError('Image hash changed')
    # Fully decode the bounded still before retaining it for a native image input.
    # Header dimensions above cap decoded allocation; animation is excluded.
    try:
        decoded=subprocess.run(['ffmpeg','-v','error','-xerror','-nostdin','-threads','1',
            '-i','pipe:0','-frames:v','1','-f','null','-'],input=data,
            stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=10,check=False)
    except subprocess.TimeoutExpired:raise ValueError('Image decoding expired') from None
    if decoded.returncode:raise ValueError('Image could not be decoded')
    directory=private_directory(Path(str(db)+'.visual'));sync_directory(directory.parent)
    fd,name=tempfile.mkstemp(prefix='.image-',dir=directory)
    try:
        with os.fdopen(fd,'wb') as stream:
            stream.write(data);stream.flush();os.fsync(stream.fileno())
        destination=directory/expected
        try:os.link(name,destination,follow_symlinks=False)
        except FileExistsError:read_png(destination,expected)
        sync_directory(directory)
    finally:os.unlink(name)
    return {'image_sha256':expected,'width':width,'height':height,'media':'retained','work_admitted':False}


class VisualSelectionInvalid(ValueError):
    """Validation failed before any original submission was attempted."""


def check_reference(value, fresh=False):
    try:
        _check_reference(value,fresh)
    except (ValueError,TypeError,KeyError) as error:
        raise VisualSelectionInvalid("Select the captured image again") from error


def _check_reference(value, fresh=False):
    # The controller validates the exact file, timestamp, selection and original
    # identity transactionally. This boundary only bounds untrusted JSON shape.
    if value is None:return
    keys={'source_kind','source_id','image_sha256','width','height','captured_at','selected_at','selected_revision','state','point'}
    if not isinstance(value,dict) or set(value)!=keys or len(json.dumps(value))>4096:
        raise ValueError('Select a retained image')

    import re
    import uuid
    from datetime import datetime,timezone
    if (value['source_kind'] not in ('selected_image','camera_frame','application_selection')
            or not isinstance(value['source_id'],str) or str(uuid.UUID(value['source_id']))!=value['source_id']
            or not isinstance(value['image_sha256'],str) or not re.fullmatch('[a-f0-9]{64}',value['image_sha256'])
            or value['selected_revision']!=value['image_sha256'] or value['state']!='selected'
            or type(value['width']) is not int or type(value['height']) is not int
            or not 0<value['width']<=4096 or not 0<value['height']<=4096 or value['width']*value['height']>4194304):
        raise ValueError('Visual reference missing, changed or ambiguous. Select the image again.')
    point=value['point']
    if point is not None and (not isinstance(point,dict) or set(point)!={'x','y'}
            or any(type(point[k]) is not int for k in ('x','y'))
            or not 0<=point['x']<value['width'] or not 0<=point['y']<value['height']):
        raise ValueError('Selected point is outside the captured image.')
    try:
        captured=datetime.fromisoformat(value['captured_at'].replace('Z','+00:00'))
        selected=datetime.fromisoformat(value['selected_at'].replace('Z','+00:00'))
        if (not captured.tzinfo or not selected.tzinfo or selected<captured
                or value['source_kind']=='camera_frame' and (selected-captured).total_seconds()>5
                or fresh and not -5<=(datetime.now(timezone.utc)-selected).total_seconds()<=300):
            raise ValueError('Visual selection is stale. Select the image again.')
    except (TypeError,AttributeError,OverflowError):raise ValueError('Select one current visual reference') from None
