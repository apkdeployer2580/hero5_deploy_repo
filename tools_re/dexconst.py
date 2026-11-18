import struct, sys

d = open(sys.argv[1], 'rb').read()

# --- correct dex header offsets ---
(map_off, string_ids_size, string_ids_off, type_ids_size, type_ids_off,
 proto_ids_size, proto_ids_off, field_ids_size, field_ids_off,
 method_ids_size, method_ids_off, class_defs_size, class_defs_off,
 data_size, data_off) = struct.unpack_from('<15I', d, 52)


def uleb(p):
    n = 0; shift = 0
    while True:
        b = d[p]; p += 1
        n |= (b & 0x7f) << shift; shift += 7
        if not (b & 0x80):
            return n, p


soff = struct.unpack_from('<%dI' % string_ids_size, d, string_ids_off)


def s(i):
    _, p = uleb(soff[i])
    return d[p:d.index(b'\x00', p)].decode('utf-8', 'replace')


tids = struct.unpack_from('<%dI' % type_ids_size, d, type_ids_off)
t = lambda i: s(tids[i])

mids = []
for i in range(method_ids_size):
    ci, pi, ni = struct.unpack_from('<HHI', d, method_ids_off + i * 8)
    mids.append((t(ci), s(ni)))

# --- Dalvik instruction widths, in 16-bit code units, by opcode ---
W = [1] * 256
for a, b, w in [(0x00, 0x01, 1), (0x02, 0x02, 2), (0x03, 0x03, 3), (0x04, 0x04, 1),
                (0x05, 0x05, 2), (0x06, 0x06, 3), (0x07, 0x07, 1), (0x08, 0x08, 2),
                (0x09, 0x09, 3), (0x0a, 0x12, 1), (0x13, 0x13, 2), (0x14, 0x14, 3),
                (0x15, 0x16, 2), (0x17, 0x17, 3), (0x18, 0x18, 5), (0x19, 0x1a, 2),
                (0x1b, 0x1b, 3), (0x1c, 0x1c, 2), (0x1d, 0x1e, 1), (0x1f, 0x20, 2),
                (0x21, 0x21, 1), (0x22, 0x23, 2), (0x24, 0x26, 3), (0x27, 0x28, 1),
                (0x29, 0x29, 2), (0x2a, 0x2c, 3), (0x2d, 0x31, 2), (0x32, 0x3d, 2),
                (0x3e, 0x43, 1), (0x44, 0x6d, 2), (0x6e, 0x72, 3), (0x73, 0x73, 1),
                (0x74, 0x78, 3), (0x79, 0x7a, 1), (0x7b, 0x8f, 1), (0x90, 0xaf, 2),
                (0xb0, 0xcf, 1), (0xd0, 0xe2, 2), (0xe3, 0xff, 1)]:
    for o in range(a, b + 1):
        W[o] = w
for o in range(0x60, 0x6e):
    W[o] = 2

WANT = sys.argv[2:]

for i in range(class_defs_size):
    cidx, flags, sup, ifs, srcf, ann, cdata, sval = struct.unpack_from('<8I', d, class_defs_off + i * 32)
    cname = t(cidx)
    if not any(w in cname for w in WANT) or not cdata:
        continue
    print("\n" + "=" * 74)
    print("CLASS %s  extends %s" % (cname, t(sup) if sup != 0xffffffff else '?'))
    p = cdata
    nsf, p = uleb(p); nif, p = uleb(p); ndm, p = uleb(p); nvm, p = uleb(p)
    for _ in range(nsf + nif):
        _, p = uleb(p); _, p = uleb(p)
    midx = 0
    for k in range(ndm + nvm):
        if k == ndm:
            midx = 0
        di, p = uleb(p); _, p = uleb(p); code_off, p = uleb(p)
        midx += di
        _, nm = mids[midx]
        if not code_off:
            continue
        rsz, ins, outs, tries, dbg, insns = struct.unpack_from('<HHHHII', d, code_off)
        base = code_off + 16
        consts, calls, j = [], [], 0
        while j < insns:
            w = struct.unpack_from('<H', d, base + j * 2)[0]
            op = w & 0xff
            if op == 0x13:
                consts.append(struct.unpack_from('<h', d, base + (j + 1) * 2)[0])
            elif op == 0x14:
                consts.append(struct.unpack_from('<i', d, base + (j + 1) * 2)[0])
            elif op == 0x15:
                consts.append(struct.unpack_from('<h', d, base + (j + 1) * 2)[0] << 16)
            elif op in (0x6e, 0x6f, 0x70, 0x71, 0x72):
                mi = struct.unpack_from('<H', d, base + (j + 1) * 2)[0]
                if mi < len(mids):
                    calls.append("%s.%s" % (mids[mi][0].split('/')[-1].rstrip(';'), mids[mi][1]))
            j += W[op] or 1
        interesting = [c for c in consts if 32 <= abs(c) <= 4096]
        if interesting or any('nativeInit' in c or 'Clet' in c for c in calls):
            print("  -- %s" % nm)
            if interesting:
                print("       consts: %s" % interesting[:26])
            hits = [c for c in calls if 'native' in c or 'Clet' in c or 'init' in c.lower()]
            if hits:
                print("       calls : %s" % hits[:14])
