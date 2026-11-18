import struct, sys, re

# ---- binary AXML string pool ----
def axml_strings(path):
    d = open(path, 'rb').read()
    # chunk 0x001C0001 file, then 0x001C0001... string pool type 0x0001
    off = 8
    typ, hsz, sz = struct.unpack_from('<HHI', d, off)
    assert typ == 0x0001, hex(typ)
    cnt, styc, flags, sstart, sstyle = struct.unpack_from('<IIIII', d, off + 8)
    utf8 = bool(flags & (1 << 8))
    offs = struct.unpack_from('<%dI' % cnt, d, off + 28)
    base = off + sstart
    out = []
    for o in offs:
        p = base + o
        if utf8:
            n = d[p]
            if n & 0x80:
                p += 1
                n = ((n & 0x7f) << 8) | d[p]
            p += 1
            n2 = d[p]
            if n2 & 0x80:
                p += 1
                n2 = ((n2 & 0x7f) << 8) | d[p]
            p += 1
            out.append(d[p:p + n2].decode('utf-8', 'replace'))
        else:
            n = struct.unpack_from('<H', d, p)[0]
            if n & 0x8000:
                n = ((n & 0x7fff) << 16) | struct.unpack_from('<H', d, p + 2)[0]
                p += 2
            p += 2
            out.append(d[p:p + n * 2].decode('utf-16-le', 'replace'))
    return out


# ---- dex string ids ----
def dex_strings(path):
    d = open(path, 'rb').read()
    str_ids_size, str_ids_off = struct.unpack_from('<II', d, 56)
    offs = struct.unpack_from('<%dI' % str_ids_size, d, str_ids_off)
    out = []
    for o in offs:
        p = o
        n = 0
        shift = 0
        while True:
            b = d[p]; p += 1
            n |= (b & 0x7f) << shift
            shift += 7
            if not (b & 0x80):
                break
        e = d.index(b'\x00', p)
        out.append(d[p:e].decode('utf-8', 'replace'))
    return out


print("=== AndroidManifest string pool ===")
for s in axml_strings(sys.argv[1]):
    print("   " + s)

print("\n=== dex strings: classes / natives / interesting ===")
ds = dex_strings(sys.argv[2])
print("total dex strings: %d" % len(ds))
pat = re.compile(r'(eamobile|Clet|WIPI|native|Activity|SKT|Midas|MIDAS|vfs|VFS|Asset|touch|Touch|GL|GLSurface)')
for s in ds:
    if pat.search(s):
        print("   " + s)
