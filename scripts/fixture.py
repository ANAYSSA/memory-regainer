#!/usr/bin/env python3
"""Create a disposable FAT32 recovery lab, never a real device.
Usage: python3 scripts/fixture.py /path/to/new/lab-directory
The directory must not already exist. No personal content is used.
"""
import hashlib, json, struct, sys
from pathlib import Path

if len(sys.argv) != 2:
    raise SystemExit(__doc__)
out = Path(sys.argv[1]).resolve()
out.mkdir(parents=True, exist_ok=False)
fixtures = Path(__file__).resolve().parent.parent/'crates'/'recovery-core'/'tests'/'fixtures'
files = [('MEMORY  JPG', 'original.jpg', (fixtures/'original.jpg').read_bytes()),
         ('PHOTO   PNG', 'original.png', (fixtures/'original.png').read_bytes()),
         ('NOTES   TXT', 'original.txt', 'Memory Regainer recovery lab.\nВаши воспоминания важны.\n'.encode())]
sector = 512
clusters = 65526
reserved = 32
fat_sectors = (clusters+2)*4//512+1
sectors = reserved+fat_sectors+clusters
fat_at = reserved*sector
data_at = (reserved+fat_sectors)*sector
b = bytearray(512)
b[:3] = b'\xeb\x58\x90';b[3:11] = b'MEMREGEN'
struct.pack_into('<H',b,11,512);b[13] = 1
struct.pack_into('<H',b,14,reserved);b[16] = 1;b[21] = 0xf8
struct.pack_into('<I',b,32,sectors);struct.pack_into('<I',b,36,fat_sectors)
struct.pack_into('<I',b,44,2);struct.pack_into('<HH',b,48,1,6)
b[64] = 0x80;b[66] = 0x29;struct.pack_into('<I',b,67,0x4d524c42)
b[71:82] = b'MEMORY LAB ';b[82:90] = b'FAT32   ';b[510:512] = b'\x55\xaa'
fsi = bytearray(512)
struct.pack_into('<I',fsi,0,0x41615252);struct.pack_into('<III',fsi,484,0x61417272,0xffffffff,0xffffffff);struct.pack_into('<I',fsi,508,0xaa550000)
manifest = {'description':'Three genuinely deleted records: first short-name byte erased and allocation chain cleared; original payload remains in free contiguous clusters. Sparse ordinary FAT32 file, no device access.', 'filesystem':'FAT32', 'image':'demo-fat32.img', 'files':[]}
with (out/'demo-fat32.img').open('xb') as image:
    image.truncate(sectors*sector)
    for at, block in [(0,b),(512,fsi),(6*512,b),(7*512,fsi),(fat_at,struct.pack('<III',0x0ffffff8,0x0fffffff,0x0fffffff))]:
        image.seek(at);image.write(block)
    first = 3
    for index,(short,name,payload) in enumerate(files):
        record = bytearray(32);record[:11] = short.encode();record[0] = 0xe5;record[11] = 0x20
        struct.pack_into('<H',record,26,first);struct.pack_into('<I',record,28,len(payload))
        image.seek(data_at+index*32);image.write(record)
        offset = data_at+(first-2)*sector;image.seek(offset);image.write(payload)
        (out/name).write_bytes(payload)
        manifest['files'].append({'original':name,'recovered_short_name':'_'+short[1:8].rstrip()+'.'+short[8:].rstrip(),'bytes':len(payload),'offset':offset,'sha256':hashlib.sha256(payload).hexdigest()})
        first += (len(payload)+511)//512
manifest['image_bytes'] = sectors*sector
(out/'expected.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({'image':str(out/'demo-fat32.img'),'expected':str(out/'expected.json'),'files':manifest['files']},ensure_ascii=False,indent=2))
