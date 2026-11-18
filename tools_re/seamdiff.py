import re, sys
from elftools.elf.elffile import ELFFile


def imports(path):
    f = open(path, 'rb')
    elf = ELFFile(f)
    dyn = elf.get_section_by_name('.dynsym')
    out = set()
    for s in dyn.iter_symbols():
        if s['st_shndx'] == 'SHN_UNDEF' and s.name:
            out.add(s.name)
    return out


def jni_entries(path):
    f = open(path, 'rb')
    elf = ELFFile(f)
    dyn = elf.get_section_by_name('.dynsym')
    return {s.name for s in dyn.iter_symbols()
            if s.name.startswith('Java_') and s['st_shndx'] != 'SHN_UNDEF'}


def callbacks(path):
    """native->Java: name string immediately preceding a JNI descriptor in .rodata"""
    f = open(path, 'rb')
    elf = ELFFile(f)
    ro = elf.get_section_by_name('.rodata')
    data, base = ro.data(), ro['sh_addr']
    strings, start = [], None
    for i, b in enumerate(data):
        if 32 <= b < 127:
            if start is None:
                start = i
        else:
            if start is not None and i - start >= 2:
                strings.append(data[start:i].decode('ascii'))
            start = None
    sigre = re.compile(r'^\([A-Za-z\[;/0-9_$]*\)[A-Za-z\[;/0-9_$]+$')
    idre = re.compile(r'^[A-Za-z_][A-Za-z0-9_]{1,60}$')
    out = {}
    for i, s in enumerate(strings):
        if sigre.match(s) and i and idre.match(strings[i - 1]):
            out[strings[i - 1]] = s
    return out


A, B = sys.argv[1], sys.argv[2]
NA, NB = sys.argv[3], sys.argv[4]


def show(title, a, b):
    print("\n" + "=" * 72)
    print("  " + title)
    print("=" * 72)
    print("  %s only (%d):" % (NA, len(a - b)))
    for x in sorted(a - b):
        print("      " + x)
    print("  %s only (%d):" % (NB, len(b - a)))
    for x in sorted(b - a):
        print("      " + x)
    print("  shared: %d   |  %s total %d  |  %s total %d"
          % (len(a & b), NA, len(a), NB, len(b)))


show("IMPORTS", imports(A), imports(B))
show("JNI ENTRY POINTS (Java -> native)", jni_entries(A), jni_entries(B))
ca, cb = callbacks(A), callbacks(B)
show("JAVA CALLBACKS (native -> Java)", set(ca), set(cb))
print("\n  signature mismatches on shared callbacks:")
mism = [(k, ca[k], cb[k]) for k in sorted(set(ca) & set(cb)) if ca[k] != cb[k]]
for k, x, y in mism:
    print("      %-32s %s   vs   %s" % (k, x, y))
if not mism:
    print("      (none — every shared callback has an identical signature)")
