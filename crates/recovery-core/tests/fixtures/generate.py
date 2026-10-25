#!/usr/bin/env python3
"""Regenerate original, non-sensitive binary parser fixtures using the standard library.
original.jpg is generated independently by the adjacent macOS Swift script.
"""
import io, struct, zlib, zipfile, wave, hashlib, json
from pathlib import Path
p = Path(__file__).parent
chunk = lambda name, data: struct.pack('>I', len(data)) + name + data + struct.pack('>I', zlib.crc32(name+data))
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 2, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(b'\0\xff\0\0\0\xff\0\0\0\0\xff\xff\xff\xff')) + chunk(b'IEND', b'')
(p/'original.png').write_bytes(png)
pdf = b'%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n'
o2=len(pdf);pdf+=b'2 0 obj\n<< /Type /Pages /Count 0 /Kids [] >>\nendobj\n'
xref=len(pdf);pdf+=b'xref\n0 3\n0000000000 65535 f \n0000000009 00000 n \n'+f'{o2:010d} 00000 n \n'.encode()+b'trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n'+str(xref).encode()+b'\n%%EOF\n'
(p/'original.pdf').write_bytes(pdf)
# One white pixel, a complete original GIF89a stream.
(p/'original.gif').write_bytes(b'GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff\x2c\0\0\0\0\x01\0\x01\0\0\x02\x02\x4c\x01\0\x3b')
bmp=b'BM'+struct.pack('<IHHI',58,0,0,54)+struct.pack('<IiiHHIIiiII',40,1,1,1,24,0,4,2835,2835,0,0)+b'\0\0\xff\0'
(p/'original.bmp').write_bytes(bmp)
b=io.BytesIO()
with zipfile.ZipFile(b,'w',compression=zipfile.ZIP_STORED) as z:
 info=zipfile.ZipInfo('memory.txt',date_time=(2026,1,1,0,0,0));z.writestr(info,b'Original recovery test payload.\n')
(p/'original.zip').write_bytes(b.getvalue())
b=io.BytesIO()
with wave.open(b,'wb') as w:
 w.setnchannels(1);w.setsampwidth(2);w.setframerate(8000);w.writeframes(struct.pack('<8h',0,1000,2000,1000,0,-1000,-2000,-1000))
(p/'original.wav').write_bytes(b.getvalue())
hashes={f.name: hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(p.glob('original.*'))}
(p/'SHA256.json').write_text(json.dumps(hashes,indent=2)+'\n')
