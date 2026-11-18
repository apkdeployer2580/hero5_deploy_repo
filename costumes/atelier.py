#!/usr/bin/env python3
"""Preview a costume design on the game's own sheet, at game size and zoomed.

    python costumes/atelier.py preview witch_hat out/witch.png      # one design
    python costumes/atelier.py preview all out/costumes.png          # every design
    python costumes/atelier.py sheet witch_hat out/dir               # the composed
                                                                     # images as PNGs

Reads the base sheet from payload/data.vfs.mp3 through tools_re/mgr.py, so
nothing of the game's art is stored here. The compositing rule is the one
hl5_core::costume::compose implements — `.` keeps the base pixel, `-` clears
it, base rows above `clear_above` are dropped, and the canvas grows by the
same amount on both sides so the image centre (where the .cif puts it) does
not move. If the two ever disagree, the Rust side is what the phone draws.
"""
import sys
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "tools_re"))
import mgr  # noqa: E402

BASES = {i: f"c/sp/img{i}/000.mgr" for i in range(4)}


def hexrgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def load(slug):
    mod = {}
    exec((HERE / "sets" / f"{slug}.py").read_text(encoding="utf-8"), mod)
    return mod


def catalog():
    rows = []
    for line in (HERE / "catalog.txt").read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            cid, layer, sheet, slug = line.split()
            rows.append((int(cid), int(layer), int(sheet), slug))
    return rows


def base_images(layer, source=None):
    _, data = mgr.load_source(source or BASES[layer])
    out = []
    for g in mgr.mgr_decode(data):
        out.append(mgr.gbm_to_png_image(g, g.palette).convert("RGBA"))
    return out


def compose(base, spec, colours):
    ox, oy = spec["at"]
    grid = spec["grid"]
    gw, gh = max(len(r) for r in grid), len(grid)
    bw, bh = base.size
    padx = max(-min(0, ox), max(0, ox + gw - bw))
    pady = max(-min(0, oy), max(0, oy + gh - bh))
    out = Image.new("RGBA", (bw + 2 * padx, bh + 2 * pady), (0, 0, 0, 0))
    clear = spec.get("clear_above", -999)
    for y in range(bh):
        if y < clear:
            continue
        for x in range(bw):
            p = base.getpixel((x, y))
            if p[3] >= 128:
                out.putpixel((x + padx, y + pady), p)
    for gy, row in enumerate(grid):
        for gx, ch in enumerate(row):
            if ch == ".":
                continue
            at = (ox + gx + padx, oy + gy + pady)
            out.putpixel(at, (0, 0, 0, 0) if ch == "-" else (*colours[ch], 255))
    return out


def composed(slug, layer=0):
    m = load(slug)
    colours = {k: hexrgb(v) for k, v in m["COLOURS"].items()}
    bases = base_images(layer, m.get("BASE"))
    return bases, [
        compose(b, m["FRAMES"][i], colours) if i in m["FRAMES"] else b for i, b in enumerate(bases)
    ]


def preview_one(slug, scale=8):
    bases, ims = composed(slug)
    shown = [i for i, b in enumerate(bases) if b.width > 1]
    cell_w = max(ims[i].width for i in shown) * scale + 12
    cell_h = max(ims[i].height for i in shown) * scale
    top = max(bases[i].height for i in shown) * scale
    sheet = Image.new("RGBA", (cell_w * len(shown), top + cell_h + 16), (90, 96, 110, 255))
    for k, i in enumerate(shown):
        b = bases[i].resize((bases[i].width * scale, bases[i].height * scale), Image.NEAREST)
        a = ims[i].resize((ims[i].width * scale, ims[i].height * scale), Image.NEAREST)
        x = k * cell_w
        sheet.alpha_composite(b, (x + (cell_w - b.width) // 2, 0))
        sheet.alpha_composite(a, (x + (cell_w - a.width) // 2, top + 16))
    return sheet


def main():
    cmd, slug, out = sys.argv[1:4]
    if cmd == "preview":
        # "all" is the hand-drawn costumes; the astra sets (rig poses, weapons,
        # effects) are previewed by astra_look.py / astra_preview.py.
        slugs = [r[3] for r in catalog() if not r[3].startswith("astra_")] if slug == "all" else [slug]
        sheets = [preview_one(s) for s in slugs]
        w = max(s.width for s in sheets)
        h = sum(s.height for s in sheets)
        page = Image.new("RGBA", (w, h), (90, 96, 110, 255))
        y = 0
        for s in sheets:
            page.alpha_composite(s, (0, y))
            y += s.height
        Path(out).parent.mkdir(parents=True, exist_ok=True)
        page.save(out)
    elif cmd == "sheet":
        _, ims = composed(slug)
        d = Path(out)
        d.mkdir(parents=True, exist_ok=True)
        for i, im in enumerate(ims):
            im.save(d / f"{i:03d}.png")
    else:
        sys.exit(f"unknown command {cmd}")


if __name__ == "__main__":
    main()
