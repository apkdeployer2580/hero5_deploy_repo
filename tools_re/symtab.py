"""Dump the guest binary's .symtab as two sorted tables.

    python tools_re/symtab.py            # regenerates guest_functions.txt / guest_objects.txt

Every other tool here keys off these files, and so does every profile and
fault report the runtime prints: `libHeroesLore5.so` shipped with its symbol
table intact, so the host never has to talk about bare addresses.
"""
import os
import sys

from elftools.elf.elffile import ELFFile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SO = os.path.join(ROOT, 'payload', 'libHeroesLore5.so')


def dump(kind, out_path):
    elf = ELFFile(open(SO, 'rb'))
    st = elf.get_section_by_name('.symtab')
    rows = [(s['st_value'], s['st_size'], s.name)
            for s in st.iter_symbols()
            if s['st_info']['type'] == kind and s.name]
    rows.sort()
    with open(out_path, 'w', encoding='utf-8') as f:
        for addr, size, name in rows:
            f.write('%08x %6d %s\n' % (addr, size, name))
    return len(rows)


if __name__ == '__main__':
    n = dump('STT_FUNC', os.path.join(HERE, 'guest_functions.txt'))
    m = dump('STT_OBJECT', os.path.join(HERE, 'guest_objects.txt'))
    print('guest_functions.txt: %d functions' % n)
    print('guest_objects.txt:   %d objects' % m)
