#!/usr/bin/env python3
"""Original GPL-3.0-or-later fixtures. Python zipfile and documented stored RAR
records only; no third-party files or RAR encoder/library code. Runtime/CI does
not execute this generator or require archive tools.
"""
from pathlib import Path
import io, zipfile, struct, zlib, json, hashlib, subprocess
P=Path(__file__).parent
DATA=b'Original Memory Regainer archive fixture.\n'
class Streaming(io.BytesIO):
    def seekable(self): return False
    def seek(self,*args): raise io.UnsupportedOperation('streaming output')
def zip_bytes(files,stream=False,force64=False,method=zipfile.ZIP_DEFLATED):
    b=Streaming() if stream else io.BytesIO()
    with zipfile.ZipFile(b,'w',compression=zipfile.ZIP_DEFLATED) as z:
        for name,data in files:
            i=zipfile.ZipInfo(name,date_time=(2026,1,1,0,0,0));i.compress_type=method
            if force64:
                with z.open(i,'w',force_zip64=True) as f:f.write(data)
            else:z.writestr(i,data)
    return b.getvalue()
files={'classic.zip':zip_bytes([('memory.txt',DATA)]),'descriptor.zip':zip_bytes([('memory.txt',DATA)],True),'descriptor-zip64.zip':zip_bytes([('memory.txt',DATA)],True,True)}
s=files['descriptor.zip']; dd=s.index(b'PK\x07\x08'); s=s[:dd]+s[dd+4:]
e=s.index(b'PK\x05\x06');s=bytearray(s);struct.pack_into('<I',s,e+16,struct.unpack_from('<I',s,e+16)[0]-4);files['descriptor-unsigned.zip']=bytes(s)
# Fully ZIP64 central/end records even for a small original stored payload.
name=b'memory.txt';crc=zlib.crc32(DATA)
extra=struct.pack('<HHQQ',1,16,len(DATA),len(DATA))
local=struct.pack('<4s5H3I2H',b'PK\x03\x04',45,0,0,0,0,crc,0xffffffff,0xffffffff,len(name),len(extra))+name+extra+DATA
extra_c=struct.pack('<HHQQQ',1,24,len(DATA),len(DATA),0)
central=struct.pack('<4s6H3I5H2I',b'PK\x01\x02',45,45,0,0,0,0,crc,0xffffffff,0xffffffff,len(name),len(extra_c),0,0,0,0,0xffffffff)+name+extra_c
z64=struct.pack('<4sQ2H2I4Q',b'PK\x06\x06',44,45,45,0,0,1,1,len(central),len(local))
locator=struct.pack('<4sIQI',b'PK\x06\x07',0,len(local)+len(central),1)
end=struct.pack('<4s4H2IH',b'PK\x05\x06',0,0,65535,65535,0xffffffff,0xffffffff,0)
files['full-zip64.zip']=local+central+z64+locator+end
content=b'''<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>'''
rels=b'''<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'''
doc=b'''<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Original test document.</w:t></w:r></w:p></w:body></w:document>'''
parts=[('[Content_Types].xml',content),('_rels/.rels',rels),('word/document.xml',doc)]
files['original.docx']=zip_bytes(parts)
files['descriptor.docx']=zip_bytes(parts,True)
files['names-only.zip']=zip_bytes([(n,b'Not an OOXML document') for n,_ in parts])
files['external-main.zip']=zip_bytes([(n, data.replace(b'Target="word/document.xml"',b'Target="https://invalid.example/document.xml" TargetMode="External"') if n=='_rels/.rels' else data) for n,data in parts])
files['doctype.zip']=zip_bytes([(n,data.replace(b'?>',b'?><!DOCTYPE Types [<!ENTITY x "no">]>',1) if n=='[Content_Types].xml' else data) for n,data in parts])
files['embedded-end.zip']=zip_bytes([('memory.txt',b'Original prefix PK\x05\x06'+b'\0'*30+b'original suffix')],method=zipfile.ZIP_STORED)
files['oversized-docx-metadata.zip']=zip_bytes([(n,doc.replace(b'Original test document.',b'x'*(1024*1024)) if n=='word/document.xml' else data) for n,data in parts])
# RAR4 stored file, finite headers and explicit end; CRC32 truncated for headers.
def r4(kind,flags,body):
    h=struct.pack('<BHH',kind,flags,7+len(body))+body
    return struct.pack('<H',zlib.crc32(h)&65535)+h
r4main=r4(0x73,0,b'\0'*6)
r4file=r4(0x74,0x8000,struct.pack('<IIBIIBBHI',len(DATA),len(DATA),3,crc,0,20,0x30,len(name),0x81a4)+name)
files['stored-rar4.rar']=b'Rar!\x1a\x07\0'+r4main+r4file+DATA+r4(0x7b,0,b'')
def vint(n):
    b=bytearray()
    while n>=128:b.append((n&127)|128);n>>=7
    b.append(n);return bytes(b)
def r5(body):
    raw=vint(len(body))+body
    return struct.pack('<I',zlib.crc32(raw))+raw
main=r5(vint(1)+vint(0)+vint(0))
header=r5(vint(2)+vint(2)+vint(len(DATA))+vint(4)+vint(len(DATA))+vint(0x81a4)+struct.pack('<I',crc)+vint(0)+vint(1)+vint(len(name))+name)
files['stored-rar5.rar']=b'Rar!\x1a\x07\x01\0'+main+header+DATA+r5(vint(5)+vint(0)+vint(0))
rows=[]
for name,data in files.items():
    (P/name).write_bytes(data)
    if name.endswith(('.zip','.docx')):
        with zipfile.ZipFile(io.BytesIO(data)) as z:assert z.testzip() is None
    row={'file':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
    if name.startswith('stored-rar'):
        result=subprocess.run(['/usr/bin/bsdtar','-xOf',str(P/name),'memory.txt'],capture_output=True)
        assert result.returncode==0,(name,result.stderr)
        assert result.stdout==DATA
        row['independent_bsdtar_extract_exit_code']=0
        row['payload_sha256']=hashlib.sha256(result.stdout).hexdigest()
    rows.append(row)
(P/'manifest.json').write_text(json.dumps({'schema_version':1,'license':'GPL-3.0-or-later','origin':'Original synthetic ZIP/DOCX and documented stored RAR4/RAR5 framing','files':rows},indent=2)+'\n')
print(json.dumps(rows,indent=2))
