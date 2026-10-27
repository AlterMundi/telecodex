"""Opaque original bytes, not an uploaded document execution test."""
import hashlib
from pathlib import Path
import sys
import tempfile
import uuid
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from dashboard.files import check_reference,retain_file,read_file

def rejects(action):
    try:action()
    except (ValueError,OSError):return
    raise AssertionError('Invalid original was accepted')

with tempfile.TemporaryDirectory() as temp:
    db=Path(temp)/'receiver.sqlite'
    for name,mime,raw in [('sketch.pdf','application/pdf',b'%PDF-1.4\nscanned fixture\n%%EOF'),('notes.txt','text/plain','una nota\n'.encode()),('payload.bin','application/octet-stream',bytes(range(256))),('empty.txt','text/plain',b'')]:
        ref={'file_id':str(uuid.uuid4()),'name':name,'mime':mime,'size':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
        ack=retain_file(db,raw,ref);assert ack['work_admitted'] is False and read_file(db,ref)==raw
        assert retain_file(db,raw,ref)==ack
        rejects(lambda:read_file(db,{**ref,'size':ref['size']+1}))
        rejects(lambda:retain_file(db,raw+b'x',ref))
        rejects(lambda:check_reference([{**ref,'name':'../escape.pdf'}]))
        rejects(lambda:check_reference([{**ref,'mime':'text/html\r\nx: injected'}]))
        rejects(lambda:check_reference([ref,ref]))
        if raw:
            path=Path(str(db)+'.files')/ref['sha256'];path.write_bytes(b'x'*len(raw));rejects(lambda:read_file(db,ref));path.unlink();path.symlink_to(Path(temp)/'elsewhere');rejects(lambda:read_file(db,ref));path.unlink()
    print('PASS original PDF/text/binary/empty bytes, idempotent receipt, bounds, changed source and symlink rejection; no model work admitted')

# A full receiving store stops growth, preserves originals and accepts exact retry.
from unittest.mock import patch
from dashboard import files
with tempfile.TemporaryDirectory() as temp:
    db=Path(temp)/'bounded.sqlite';raw=b'original'
    ref={'file_id':str(uuid.uuid4()),'name':'original.bin','mime':'application/octet-stream','size':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
    retain_file(db,raw,ref)
    other=b'next';next_ref={**ref,'file_id':str(uuid.uuid4()),'size':len(other),'sha256':hashlib.sha256(other).hexdigest()}
    with patch.object(files,'MAX_STORED_BYTES',len(raw)):
        rejects(lambda:retain_file(db,other,next_ref));assert read_file(db,ref)==raw
        assert retain_file(db,raw,ref)['work_admitted'] is False
    with patch.object(files,'MAX_STORED_FILES',1):rejects(lambda:retain_file(db,other,next_ref))
    assert len(list(Path(str(db)+'.files').glob('[a-f0-9]'*64)))==1
print('PASS receiving storage byte/count caps, unchanged originals and identical retry at capacity')
