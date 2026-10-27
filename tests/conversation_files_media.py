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
