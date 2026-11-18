"""Extract from `payload/libHeroesLore5.so.idb` the two things the ELF symbol
table does NOT already give us.

    pip install python-idb
    python tools_re/idb_extract.py

The .idb shipped inside the original APK by accident. It turned out to be plain
IDA 7.3 auto-analysis — see ANALYSIS.md "IDA 데이터베이스" — so almost everything
in it duplicates `.symtab`. Two things do not:

  1. function boundaries IDA found where `.symtab` names nothing.
     Without these a guest fault at such an address prints a bare number.
  2. struct layouts for the WIPI/midas seam types.

Writes `idb_extra_functions.txt` and `idb_structs.txt` next to this script.
"""
import os
import sys

import idb
import idb.netnode
from elftools.elf.elffile import ELFFile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SO = os.path.join(ROOT, 'payload', 'libHeroesLore5.so')
IDB = os.path.join(ROOT, 'payload', 'libHeroesLore5.so.idb')

# libgcc/EABI aliases: IDA prefers one spelling, the symbol table the other.
# Same code at the same address, so not a disagreement worth reporting.
ALIASES = {
    ('__aeabi_dadd', '__adddf3'), ('__floatunsidf', '__aeabi_ui2d'),
    ('__extendsfdf2', '__aeabi_f2d'), ('__nedf2', '__cmpdf2'),
    ('__truncdfsf2', '__aeabi_d2f'), ('__floatunsisf', '__aeabi_ui2f'),
    ('__floatundisf', '__aeabi_ul2f'), ('__floatdisf', '__aeabi_l2f'),
    ('__mulsf3', '__aeabi_fmul'), ('__divsf3', '__aeabi_fdiv'),
    ('__gtsf2', '__gesf2'), ('__ltsf2', '__lesf2'), ('__nesf2', '__cmpsf2'),
    ('___Unwind_RaiseException', '_Unwind_RaiseException'),
    ('___Unwind_Resume', '_Unwind_Resume'),
}


def symtab_funcs():
    elf = ELFFile(open(SO, 'rb'))
    st = elf.get_section_by_name('.symtab')
    out = {}
    for s in st.iter_symbols():
        if s.name and s['st_value'] and s['st_info']['type'] == 'STT_FUNC':
            out.setdefault(s['st_value'] & ~1, s.name)
    return out


def main():
    if not os.path.exists(IDB):
        sys.exit('missing %s — see README "준비물"' % IDB)
    sym = symtab_funcs()

    with idb.from_file(IDB) as db:
        api = idb.IDAPython(db)

        # ---- 1. functions the symbol table does not name ----
        extra, disagree = [], []
        for ea in api.idautils.Functions():
            n = api.idc.GetFunctionName(ea)
            real = sym.get(ea & ~1)
            if real is None:
                extra.append((ea, n))
            elif real != n and n != real + '_0' and n != 'j_' + real:
                if (n, real) not in ALIASES:
                    disagree.append((ea, n, real))

        path = os.path.join(HERE, 'idb_extra_functions.txt')
        with open(path, 'w', encoding='utf-8') as f:
            f.write('# function starts IDA found that .symtab does not name.\n')
            f.write('# %d entries. addr  ida_name\n\n' % len(extra))
            for ea, n in sorted(extra):
                f.write('%08x %s\n' % (ea, n))
            if disagree:
                f.write('\n# names that genuinely disagree (not a known alias):\n')
                for ea, n, real in sorted(disagree):
                    f.write('%08x ida=%s symtab=%s\n' % (ea, n, real))
        print('idb_extra_functions.txt: %d functions, %d real disagreements'
              % (len(extra), len(disagree)))

        # ---- 2. struct layouts ----
        nn = idb.netnode.Netnode(db, '$ structs')
        structs = {}
        for a in nn.alts():
            try:
                raw = nn.altval(a)
            except Exception:
                continue
            sid = (raw & 0xFFFFFFFF) | 0xFF000000 if raw < 0 else raw
            try:
                full = idb.netnode.Netnode(db, sid).name()
            except Exception:
                continue
            if not full or '.' not in full:
                continue
            sname, _, member = full.partition('.')
            structs.setdefault(sname, []).append((sid, member))

        # The struct's base netnode IS its first member (its name reads
        # "Struct.field"), and the remaining members follow at consecutive ids.
        # The next struct's base bounds the walk: when a struct's members fill
        # that whole gap the list is likely cut short, and we say so rather than
        # presenting it as complete.
        bases = sorted(min(s for s, _ in v) for v in structs.values())
        for sname, first in list(structs.items()):
            base = min(s for s, _ in first)
            nxt = next((b for b in bases if b > base), base + 64)
            room = nxt - base
            members = []
            for off in range(0, min(room, 64)):
                try:
                    nm = idb.netnode.Netnode(db, base + off).name()
                except Exception:
                    continue
                if nm and nm.startswith(sname + '.'):
                    members.append(nm.split('.', 1)[1])
            structs[sname] = (members, len(members) >= room and room > 0)

        path = os.path.join(HERE, 'idb_structs.txt')
        with open(path, 'w', encoding='utf-8') as f:
            f.write('# Struct field NAMES recovered from the .idb, in order.\n'
                    '# NOT offsets, NOT types, NOT sizes — confirm those by reading the\n'
                    '# access code with disasm.py before relying on any layout.\n'
                    '#\n'
                    '# Most entries are IDA standard types (Elf32_*, sockaddr_in, stat,\n'
                    '# ARM unwind). The WIPI/midas seam types are the reason this file\n'
                    '# exists: MC_GrpContext, midasAudioInfo, MC_FileInfo.\n'
                    '#\n'
                    '# Recovery is heuristic (walk the member netnodes that follow the\n'
                    '# struct node), so a list can be SHORT even without the mark below:\n'
                    '# sockaddr_in comes out with 2 of its 4 fields. Treat every list as\n'
                    '# "at least these fields, in this order".\n'
                    '# "TRUNCATED?" additionally marks a struct whose members filled all\n'
                    '# the netnode space before the next struct.\n\n')
            for sname in sorted(structs):
                members, cut = structs[sname]
                f.write('%s%s\n' % (sname, '   # TRUNCATED?' if cut else ''))
                for m in members:
                    f.write('    %s\n' % m)
                f.write('\n')
        cut_n = sum(1 for _, c in structs.values() if c)
        print('idb_structs.txt: %d structs (%d possibly truncated)'
              % (len(structs), cut_n))

        SEAM = ('MC_GrpContext', 'MC_FileInfo', 'midasAudioInfo', 'MCTimer',
                'CommonEle', 'Heckbert_Edge', 'Point2', 'Barycoord')
        print('\nseam-relevant structs:')
        for s in SEAM:
            if s in structs:
                members, cut = structs[s]
                print('  %-16s%s %s' % (s, ' (TRUNCATED?)' if cut else '',
                                        ', '.join(members) or '<no members>'))


if __name__ == '__main__':
    main()
