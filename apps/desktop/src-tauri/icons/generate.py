"""Generate the built-in Entwurf E icon using only the Python standard library."""
from pathlib import Path
import struct
import zlib

root = Path(__file__).resolve().parent
size = 256
background = bytes((255, 255, 255, 255))
accent = bytes((143, 53, 255, 255))
rows = []
for y in range(size):
    row = bytearray(b'\x00')
    for x in range(size):
        vertical = 60 <= x < 88 and 48 <= y < 208
        horizontal = 60 <= x < 196 and (48 <= y < 76 or 116 <= y < 144 or 180 <= y < 208)
        row.extend(accent if vertical or horizontal else background)
    rows.append(row)

def chunk(kind, data):
    return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))

png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0))
png += chunk(b'IDAT', zlib.compress(b''.join(rows))) + chunk(b'IEND', b'')
(root / 'icon.png').write_bytes(png)
# Vista and later support a PNG-compressed 256px ICO frame.
ico = struct.pack('<HHH', 0, 1, 1) + struct.pack('<BBBBHHII', 0, 0, 0, 0, 1, 32, len(png), 22) + png
(root / 'icon.ico').write_bytes(ico)
