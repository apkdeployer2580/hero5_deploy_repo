"""Every monster drop path in Heroes Lore 5, from the shipped tables.

Bands (Monster::SetDropItem 0xBC910, one Rand(0,65535) down seven thresholds):

    [0,T0)   kind 13 material, index = enemy+0x271
    [T0,T1)  kind = Rand(0,9), index = enemy+0x272, grade FORCED 0
    [T1,T2)  same, grade FORCED 1
    [T2,T3)  same, grade FORCED 2
    [T3,T4)  droptable body[5],body[6]   table's own grade  (영웅)
    [T4,T5)  droptable body[7],body[8]   table's own grade  (전설)
    [T5,T6)  droptable body[9],body[10]  table's own grade  (세트)
    else     nothing

kind 4 (스태프) and 0xFF are dropped on the floor by the guest (0xBCCD8).
"""
import hl5data as H

DIFF = {0: "노말", 1: "하드", 2: "헬"}
BAND_OFF = {4: 5, 5: 7, 6: 9}          # band -> body offset of its (kind,index)
BASE_KINDS = [0, 1, 2, 3, 5, 6, 7, 8, 9]   # Rand(0,9); 4 is skipped by the guest

# The extra walks the host itself takes. Read back out of the Rust that takes
# them instead of copied here, because each one decides a number this page
# publishes as fact; see `hl5data.rust_ints`.
#
# There is no rate lever any more. An earlier build widened the 전설 band and
# it was withdrawn on 2026-09-07 — drop rates are the game's own on both
# content lines, and `rates::restore_shipped_drop_bands` deletes the override
# on every boot (BRANCHES.md).
BOSS_GROUPS = H.rust_range("legendaries.rs", "BOSS_GROUPS")
BOSS_ROLLS = H.rust_ints("legendaries.rs", "BOSS_ROLLS")[0]
JIN_INDEX = H.rust_ints("legendaries.rs", "JIN_INDEX")[0]
SOURCE_INDEX = H.rust_ints("legendaries.rs", "SOURCE_INDEX")[0]
SET_KINDS = H.rust_ints("legendaries.rs", "SET_KINDS")
HELL_SET_ROWS = H.rust_ints("legendaries.rs", "HELL_SET_ROWS")
LOWER_SET_ROWS = H.rust_ints("legendaries.rs", "LOWER_SET_ROWS")
# bossdrop.rs — 정체모를 괴수, one flat roll per kill over four 전설 장신구.
ACCESSORY_KIND = H.rust_ints("bossdrop.rs", "ACCESSORY_KIND")[0]
ACCESSORY_ROWS = H.rust_ints("bossdrop.rs", "ACCESSORY_ROWS")
ACCESSORY_P = H.rust_ints("bossdrop.rs", "ACCESSORY_PERCENT")[0] / 100.0
# lostdrops.rs — the twenty equipment rows the shipped tables strand behind a
# zero-width band, reopened as a host extra roll. `(group, band, slot, kind,
# index)` five at a time; the band width the host uses is the monster's own
# band 3 (`T3 - T2`), so there is no constant to copy for the rate either.
_LOST = H.rust_ints("lostdrops.rs", "LOST_SLOTS")
LOST_SLOTS = [tuple(_LOST[i:i + 5]) for i in range(0, len(_LOST), 5)]
LOST_ROW_COUNT = H.rust_ints("lostdrops.rs", "LOST_ROW_COUNT")[0]
assert len({(k, i) for _, _, _, k, i in LOST_SLOTS}) == LOST_ROW_COUNT,     "lostdrops.rs: LOST_SLOTS no longer covers LOST_ROW_COUNT rows"
LOST_GROUPS = {g for g, _, _, _, _ in LOST_SLOTS}
_ROUTES = H.rust_ints("lostdrops.rs", "LOST_ROUTES")
LOST_ROUTES = {(g, b): target for g, b, target in
               zip(_ROUTES[::3], _ROUTES[1::3], _ROUTES[2::3])}

def clean(name):
    return name.strip("^'{ ").replace("{", "").strip()

def monsters():
    """(difficulty, clean name, row) for every row that actually rolls."""
    out = []
    for d in (0, 1, 2):
        for e in H.enemies(d):
            out.append((d, clean(e["name"]), e))
    return out

def band_width(T, band):
    """This band's width out of 65536, from the monster's own ladder."""
    return max(0, T[band] - T[band - 1])

def per_kill(width, frac, rolls):
    if width <= 0 or frac <= 0 or not rolls:
        return 0.0
    p = width / 65536.0 * frac
    return 1.0 - (1.0 - p) ** rolls

def build():
    """(kind,index) -> {'own': {...}, 'demoted': {...}} keyed by display name."""
    dt = H.droptable()
    rows = monsters()
    own, demoted = {}, {}
    def add(store, key, label, prob, line=None):
        # `per_kill` returns 0 exactly when that path cannot fire at all (a
        # zero-width band, no rolls). Listing it would claim the item drops.
        if prob <= 0:
            return
        e = store.setdefault(key, {})
        cur = e.get(label)
        if cur is None:
            e[label] = {"p": prob, "ln": line}
            return
        # A source available on both content lines outranks a line-specific one.
        if cur["ln"] is not None and line is None:
            cur["ln"] = None
        cur["p"] = max(cur["p"], prob)

    for d, name, e in rows:
        rolls, T, grp = e["rolls"] or 0, e["T"], e["group"]
        label = f"{DIFF[d]} {name}"
        live = rolls > 0 and T[6] > 0
        # 정체모를 괴수: the host pays the kill even when the map spawned the
        # dead twin, using the marked row's ladder (bossdrop.rs).
        if "정체모를 괴수" in name and not live:
            twin = next((x for dd, nn, x in rows
                         if dd == d and "정체모를 괴수" in nn and (x["rolls"] or 0) > 0), None)
            if twin:
                rolls, T, grp, live = twin["rolls"], twin["T"], twin["group"], True
        if not live:
            continue

        # --- table-driven bands: the item keeps its own grade ---------------
        if grp is not None and grp * 4 + 3 < len(dt):
            for band, off in BAND_OFF.items():
                w = band_width(T, band)
                if w <= 0:
                    continue
                hit = {}
                for slot in range(4):
                    b = dt[grp * 4 + slot]["body"]
                    k, i = b[off], b[off + 1]
                    if k in (0xFF, 4) or i == 0xFF:
                        continue
                    hit[(k, i)] = hit.get((k, i), 0) + 1
                for key, n in hit.items():
                    add(own, key, label, per_kill(w, n / 4.0, rolls))

        # --- bands 1..3: kind is Rand(0,9), index is the monster's own ------
        base = e["base_index"]
        if base is not None:
            w = sum(band_width(T, b) for b in (1, 2, 3))
            if w > 0:
                for k in BASE_KINDS:
                    add(demoted, (k, base), label, per_kill(w, 0.1, rolls))

        # --- host-added extra walks, hell bosses only (legendaries.rs) ------
        if grp in BOSS_GROUPS and rolls >= BOSS_ROLLS:
            w5 = band_width(T, 5)
            # The two content lines take the SAME walk and differ only in the
            # row it lands on: `main` the (진) copy, `vanilla` the shipped row
            # it was copied from. Same bosses, same band, same count.
            for slot in range(4):
                add(own, (slot, JIN_INDEX), label, per_kill(w5, 0.25, rolls), "main")
                add(own, (slot, SOURCE_INDEX), label, per_kill(w5, 0.25, rolls), "vanilla")
            for row in HELL_SET_ROWS:
                for k in SET_KINDS:
                    add(own, (k, row), label, per_kill(w5, 0.25, rolls))
            for row in LOWER_SET_ROWS:
                for k in SET_KINDS:
                    add(own, (k, row), label,
                        per_kill(w5, 0.25 / len(LOWER_SET_ROWS), rolls))

        # --- host: the stranded slots of ordinary mobs (lostdrops.rs) ------
        # Same shape as the guest's own bands 4..6, on a band the shipped
        # ladder leaves zero-width: width = this monster's band 3, the guest's
        # `Rand(0,3)` slot pick, and a drop only when that slot is one of the
        # twenty. A band the ladder already opens is left alone, which is why
        # this can never change a rate the table already publishes.
        for g, band, _slot, kk, ii in LOST_SLOTS:
            if LOST_ROUTES.get((g, band), g) != grp:
                continue
            routed = g != grp
            if not routed and band_width(T, band) > 0:
                continue
            width = band_width(T, 5 if routed and band == 5 else 3)
            add(own, (kk, ii), label, per_kill(width, 0.25, rolls))

        # --- 정체모를 괴수: flat 1% accessory (bossdrop.rs) -----------------
        if "정체모를 괴수" in name:
            for row in ACCESSORY_ROWS:
                add(own, (ACCESSORY_KIND, row), label,
                    ACCESSORY_P / len(ACCESSORY_ROWS))
    return own, demoted
