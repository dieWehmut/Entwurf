"""Generate small public PDF fixtures, using only the Python standard library.

bilingual.png is the synthetic System.Drawing/YaHei environment smoke image:
white 1100x220 canvas, text '概率论 Probability 123'. No private content.
"""
from pathlib import Path
import struct, zlib
ROOT = Path(__file__).parent

def pdf(objects, filename):
    data = b'%PDF-1.4\n%\xe2\xe3\xcf\xd3\n'
    offsets = [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(data)); data += f'{i} 0 obj\n'.encode() + obj + b'\nendobj\n'
    xref = len(data)
    data += f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode()
    data += b''.join(f'{o:010d} 00000 n \n'.encode() for o in offsets[1:])
    data += f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode()
    (ROOT / filename).write_bytes(data)

def stream(data, extra=b''):
    return b'<< /Length ' + str(len(data)).encode() + b' ' + extra + b' >>\nstream\n' + data + b'\nendstream'

pdf([
 b'<< /Type /Catalog /Pages 2 0 R >>',
 b'<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>',
 b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 7 0 R >> >> /Contents 4 0 R >>',
 stream(b'BT /F1 18 Tf 72 700 Td (PAGE ONE Probability) Tj ET'),
 b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 7 0 R >> >> /Contents 6 0 R >>',
 stream(b'BT /F1 18 Tf 72 700 Td (PAGE TWO Conditional Probability) Tj ET'),
 b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
], 'two-pages.pdf')

# PNG's predictor filter bytes are directly supported by PDF FlateDecode.
data=(ROOT/'bilingual.png').read_bytes(); at=8; compressed=b''
while at<len(data):
    length=struct.unpack('>I',data[at:at+4])[0];kind=data[at+4:at+8];body=data[at+8:at+8+length];at+=length+12
    if kind==b'IHDR': width,height,depth,color,_,_,interlaced=struct.unpack('>IIBBBBB',body)
    if kind==b'IDAT': compressed+=body
assert depth==8 and color in (2,6) and not interlaced
# Remove PNG RGBA alpha while unfiltering rows if necessary.
bpp=4 if color==6 else 3
raw=zlib.decompress(compressed);prev=bytearray(width*bpp);rgb=bytearray();pos=0
for y in range(height):
    filt=raw[pos];pos+=1;row=bytearray(raw[pos:pos+width*bpp]);pos+=width*bpp
    for i in range(len(row)):
        a=row[i-bpp] if i>=bpp else 0;b=prev[i];c=prev[i-bpp] if i>=bpp else 0
        if filt==1: predictor=a
        elif filt==2: predictor=b
        elif filt==3: predictor=(a+b)//2
        elif filt==4:
            p=a+b-c; ds=[abs(p-a),abs(p-b),abs(p-c)];predictor=[a,b,c][ds.index(min(ds))]
        else: predictor=0
        row[i]=(row[i]+predictor)%256
    for x in range(width):rgb.extend(row[x*bpp:x*bpp+3])
    prev=row
pdf([
 b'<< /Type /Catalog /Pages 2 0 R >>',
 b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
 b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 160] /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>',
 stream(b'q 612 0 0 122 0 19 cm /Im0 Do Q'),
 stream(zlib.compress(rgb), f'/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode'.encode()),
], 'scanned.pdf')

# Deliberately unreadable middle page; indexed page 3 remains readable.
pdf([
 b'<< /Type /Catalog /Pages 2 0 R >>',
 b'<< /Type /Pages /Kids [3 0 R 5 0 R 6 0 R] /Count 3 >>',
 b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 8 0 R >> >> /Contents 4 0 R >>',
 stream(b'BT /F1 18 Tf 72 700 Td (PAGE ONE) Tj ET'),
 b'null',
 b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 8 0 R >> >> /Contents 7 0 R >>',
 stream(b'BT /F1 18 Tf 72 700 Td (PAGE THREE) Tj ET'),
 b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
], 'bad-middle.pdf')
