#!/usr/bin/env python3
"""`.mgr` sprite sheets: decode, export to PNG, rebuild, and prove the round trip.

    python mgr.py info     c/sp/img1/000.mgr
    python mgr.py export   c/sp/img1/000.mgr out/img1_000          # PNGs + manifest.json
    python mgr.py build    out/img1_000/manifest.json new.mgr        # (or the directory)
    python mgr.py sheet    c/sp/img3/000.mgr out/legs0.png           # 3x contact sheet
    python mgr.py roundtrip                                          # every .mgr in the archive
    python mgr.py recolor  c/sp/img1/000.mgr out.mgr --hue 120       # or --pal 37
    python mgr.py pal      c/sp/pal/037.pal                          # a recolour table
    python mgr.py cif      c/sp/cif/001.cif [--layer 1]              # what the animation uses
    python mgr.py pose     c/sp/cif/004.cif out.png --set 1=my.mgr   # preview a costume on the hero
    python mgr.py free                                               # numbers per /c/sp/imgN/
    python mgr.py ls                                                 # every .mgr in the archive

A source is a local file if one exists at that path, otherwise an archive path
(`/c/sp/img1/000.mgr` or `c/sp/img1/000.mgr` -- the hash ignores the leading
slash) or a raw hash (`0x7d9f00de`). Under Git Bash use the `c/...` spelling or
`MSYS_NO_PATHCONV=1`: MSYS rewrites `/c/sp/...` to `C:/sp/...` before Python
sees it.

FORMAT (all little-endian; read from the guest, confirmed on all 427 sheets)

  .mgr  = GbmMerger (OpenGbmMerger 0x85800 / GetMergedGbmData 0x856C0)
          [u32 count] then count x ([u32 size][size bytes = one GBM])
          GetMergedGbmCount reads the count as s16, GetMergedGbmData as s32;
          the upper half is always 0. No compression, no index table: entry i
          is found by walking i size-prefixed blobs.

  GBM   = GbmImage::LoadImageResID 0x85370
          +0 u8  type     low nibble = bits per pixel (4 or 8; anything else
                          loads no pixels). High nibble (0 or 1) is never read
                          by the loader or the blitters; it is 1 on images that
                          contain the transparent colour, mostly. Keep it.
          +1 u8  ncolors  palette entries
          +2 u16 width
          +4 u16 height
          +6     ncolors x u16 RGB565 palette
          then   pixels, row-major, no compression:
                   8 bpp: width*height bytes
                   4 bpp: ceil(width/2) bytes per row, the EVEN x in the HIGH
                          nibble; an odd row ends with a 0 low nibble (padding
                          is 0 in every shipped image)

  Transparency is a palette COLOUR, not an index: an entry equal to 0xF81F
  (magenta) is skipped by every blitter. The loader rewrites the first magenta
  entry to the global SKIP_COLOR, which onStartApp (0x9DC58) sets to 0xF81F in
  565 mode (0x2F80F in 666), so in practice every 0xF81F entry is see-through
  and pure black 0x0000 is an ordinary opaque colour.

  .pal  = /c/sp/pal/%03d.pal, a COLOUR REMAP applied at load time
          (SPRITE_IMAGE::LoadSprImg 0xDB510 -> Graphic::SetPalette 0x58100)
          [u8 n][n x u16 from][n x u16 to]
          For each palette entry e of every GBM in the sheet, for j in 0..n:
          if e == from[j] then e = to[j]   (sequential, so chains apply).
          Selected per layer by _nSprPalFile[layer] = ItemInfo+0x152 (s16,
          < 0 = none). Same .mgr + different .pal = a recoloured item.

  .cif  = SPRITE_DATA::LoadSprData 0xDBC64 (animation; the HERO loads it with
          flag 1). Parts reference (layer, gbm index) and give the gbm's CENTRE
          (Graphic::CalcAnchor anchor 0x12 = HCENTER|VCENTER, centre = w/2,h/2
          rounded down) relative to the sprite origin. See `cif`.

HERO LAYERS (HERO::SetLoadImgPath 0x8DE58 fills _szSprFile[10][256] and
_nSprPalFile[10]; SPRITE_IMAGE::LoadSprImg loads layers 0..cif_layers-1)

  layer  file                      from                              images used
  0      /c/sp/img0/NNN.mgr head   equip slot 1 (helmet, item_05)     0..6
         (img0/000 = bare head, also when the 투구보기 option is off)
  1      /c/sp/img1/NNN.mgr torso  equip slot 2 (armour, item_06)     0..11 (0..10
         (img1/000 = bare; 3..11 are 1x1 placeholders there)            for 002-004.cif)
  2      /c/sp/img2/NNN.mgr arms   equip slot 3 (gloves, item_07)     0..12
  3      /c/sp/img3/NNN.mgr legs   equip slot 4 (boots, item_08)      0..16
  4      /c/sp/img4/NNN.mgr weapon equip slot 0 (item_00..03)         class-specific
  5      /c/sp/img5/00C.mgr        class C's EFFECT sheet (slashes,   0..51 / 43 / 50 / 43
                                   buff labels) -- not a body part
  6      /c/sp/img6/NNN.mgr shield equip slot 5 (item_09); knight only (004.cif has 7 layers)
  NNN = ItemInfo+0x150 (s16, "%03d"), palette = ItemInfo+0x152 (every shipped
  item has one, 000.pal included; only an EMPTY slot gets -1).
  Class C: 0 warrior 001.cif, 1 rogue 002.cif, 2 gunner 003.cif, 3 knight 004.cif.

A part is drawn CENTRED at its .cif offset, and the same image index is reused
across many frames -- mirrored, flipped, rotated by 90/180/270 and scaled
(effect tail) -- so a replacement must keep each index's role (which body piece,
which facing) and is safest at the original width x height.

Needs Pillow for export/build/sheet/roundtrip; info/pal/cif/free/ls do not.
"""

from __future__ import annotations

import argparse
import colorsys
import io
import json
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vfs  # noqa: E402

for _s in (sys.stdout, sys.stderr):
    if hasattr(_s, "reconfigure"):
        _s.reconfigure(encoding="utf-8", errors="replace")

TRANSPARENT = 0xF81F  # SKIP_COLOR in 565 mode

# ----------------------------------------------------------------------------
# archive access


class Archive:
    """The VFS, indexed by hash, with names for every path the guest builds."""

    def __init__(self, path: Path = vfs.DEFAULT_VFS):
        self.path = path
        self.blob = path.read_bytes()
        self.by_hash = {h: (o, s) for h, o, s in vfs.entries(self.blob)}
        self._names: dict[int, str] | None = None

    def get(self, name: str) -> bytes | None:
        h = int(name, 16) if name.lower().startswith("0x") else vfs.djb2(name)
        hit = self.by_hash.get(h)
        if hit is None:
            return None
        o, s = hit
        return self.blob[o : o + s]

    def names(self) -> dict[int, str]:
        """hash -> path for every candidate path the guest's format strings
        can produce. 1,188 of 2,189 entries resolve; only 6 of the 427 .mgr
        blobs stay nameless (they are found by structure instead)."""
        if self._names is None:
            cands = []
            for d in range(10):
                cands += [f"/c/sp/img{d}/{n:03d}.mgr" for n in range(1000)]
            for ext in ("pal", "cif", "ext"):
                cands += [f"/c/sp/{ext}/{n:03d}.{ext}" for n in range(1000)]
            for nm in ("828_sk", "ealogo", "eff", "imo", "pointer", "title"):
                cands += [f"/c/sp/imgcom/{nm}.mgr", f"/c/sp/cif/{nm}.cif", f"/c/sp/ext/{nm}.ext"]
            cands.append("/c/sp/empty/empty.mgr")
            for nm in ("gmenu", "icon", "menu", "shadow", "touch", "ui", "worldmap"):
                cands.append(f"/c/img/{nm}.mgr")
            for n in range(100):
                cands += [
                    f"/c/map_sp/ms_img{n:02d}.mgr",
                    f"/c/map_sp/fgi_img{n:02d}.mgr",
                    f"/c/par/pimg{n:02d}.mgr",
                ]
            self._names = {}
            for c in cands:
                h = vfs.djb2(c)
                if h in self.by_hash:
                    self._names[h] = c
        return self._names

    def all_mgr(self):
        """Every entry that parses as a merger, named or not: (label, bytes)."""
        names = self.names()
        out = []
        for h, (o, s) in self.by_hash.items():
            data = self.blob[o : o + s]
            if looks_like_mgr(data):
                out.append((names.get(h, f"{h:#010x}"), data))
        out.sort(key=lambda t: t[0])
        return out


_ARCHIVE: Archive | None = None


def archive() -> Archive:
    global _ARCHIVE
    if _ARCHIVE is None:
        _ARCHIVE = Archive()
    return _ARCHIVE


def load_source(src: str) -> tuple[str, bytes]:
    p = Path(src)
    if p.is_file():
        return str(p), p.read_bytes()
    data = archive().get(src)
    if data is None:
        sys.exit(f"{src}: not a file and not in {archive().path.name} (hash {vfs.djb2(src):#010x})")
    return "/" + src.lstrip("/\\") if not src.lower().startswith("0x") else src, data


# ----------------------------------------------------------------------------
# codecs


def c565_to_rgb(c: int) -> tuple[int, int, int]:
    r, g, b = (c >> 11) & 0x1F, (c >> 5) & 0x3F, c & 0x1F
    return (r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)


def rgb_to_c565(r: int, g: int, b: int) -> int:
    return ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)


@dataclass
class Gbm:
    type: int
    width: int
    height: int
    palette: list[int]
    pixels: bytes  # one palette index per pixel, row-major, len = w*h

    @property
    def bpp(self) -> int:
        return self.type & 0xF

    @staticmethod
    def decode(b: bytes) -> "Gbm":
        if len(b) < 6:
            raise ValueError(f"GBM of {len(b)} bytes is shorter than its header")
        t, n, w, h = struct.unpack_from("<BBHH", b, 0)
        pal = list(struct.unpack_from(f"<{n}H", b, 6))
        pos = 6 + 2 * n
        bpp = t & 0xF
        if bpp == 8:
            need = w * h
            px = bytes(b[pos : pos + need])
        elif bpp == 4:
            stride = (w + 1) // 2
            need = stride * h
            raw = b[pos : pos + need]
            out = bytearray(w * h)
            for y in range(h):
                row = raw[y * stride : (y + 1) * stride]
                o = y * w
                for x in range(w):
                    v = row[x >> 1]
                    out[o + x] = (v >> 4) if not (x & 1) else (v & 0xF)
                if w & 1 and row[-1] & 0xF:
                    raise ValueError("non-zero padding nibble (never seen in shipped data)")
            px = bytes(out)
        else:
            raise ValueError(f"type {t:#x}: the loader only knows 4 and 8 bpp")
        if pos + need != len(b):
            raise ValueError(f"{w}x{h} {bpp}bpp {n} colours wants {pos + need} bytes, blob has {len(b)}")
        return Gbm(t, w, h, pal, px)

    def encode(self) -> bytes:
        n = len(self.palette)
        if n > 255:
            raise ValueError(f"{n} palette entries; the count is a u8")
        if self.bpp == 4 and max(self.pixels, default=0) > 15:
            raise ValueError("4 bpp image has an index above 15")
        out = bytearray(struct.pack("<BBHH", self.type, n, self.width, self.height))
        out += struct.pack(f"<{n}H", *self.palette)
        w, h = self.width, self.height
        if self.bpp == 8:
            out += self.pixels
        else:
            stride = (w + 1) // 2
            for y in range(h):
                row = bytearray(stride)
                for x in range(w):
                    v = self.pixels[y * w + x]
                    row[x >> 1] |= (v << 4) if not (x & 1) else v
                out += row
        return bytes(out)

    def transparent_mask(self) -> list[bool]:
        return [c == TRANSPARENT for c in self.palette]


def looks_like_mgr(data: bytes) -> bool:
    if len(data) < 4:
        return False
    count = struct.unpack_from("<I", data, 0)[0]
    if not 0 < count < 5000:
        return False
    pos = 4
    for _ in range(count):
        if pos + 4 > len(data):
            return False
        size = struct.unpack_from("<I", data, pos)[0]
        if size < 6 or pos + 4 + size > len(data):
            return False
        pos += 4 + size
    return pos == len(data)


def mgr_split(data: bytes) -> list[bytes]:
    count = struct.unpack_from("<I", data, 0)[0]
    pos, blobs = 4, []
    for i in range(count):
        size = struct.unpack_from("<I", data, pos)[0]
        blobs.append(data[pos + 4 : pos + 4 + size])
        pos += 4 + size
    if pos != len(data):
        raise ValueError(f"merger ends at {pos}, file is {len(data)} bytes")
    return blobs


def mgr_join(blobs: list[bytes]) -> bytes:
    out = bytearray(struct.pack("<I", len(blobs)))
    for b in blobs:
        out += struct.pack("<I", len(b)) + b
    return bytes(out)


def mgr_decode(data: bytes) -> list[Gbm]:
    return [Gbm.decode(b) for b in mgr_split(data)]


def mgr_encode(images: list[Gbm]) -> bytes:
    return mgr_join([g.encode() for g in images])


# .pal ------------------------------------------------------------------------


def pal_decode(data: bytes) -> list[tuple[int, int]]:
    n = data[0]
    if len(data) != 1 + 4 * n:
        raise ValueError(f".pal with n={n} should be {1 + 4 * n} bytes, is {len(data)}")
    frm = struct.unpack_from(f"<{n}H", data, 1)
    to = struct.unpack_from(f"<{n}H", data, 1 + 2 * n)
    return list(zip(frm, to))


def pal_apply(palette: list[int], remap: list[tuple[int, int]]) -> list[int]:
    """Graphic::SetPalette, 565 arm (0x58298): per entry, walk the table in
    order and replace on match -- re-reading the entry each step, so a chain
    a->b, b->c turns a into c."""
    out = []
    for c in palette:
        for f, t in remap:
            if c == f:
                c = t
        out.append(c)
    return out


def load_pal_arg(arg: str | None) -> list[tuple[int, int]] | None:
    if arg is None:
        return None
    if arg.isdigit():
        arg = f"c/sp/pal/{int(arg):03d}.pal"
    _, data = load_source(arg)
    return pal_decode(data)


# ----------------------------------------------------------------------------
# PNG side


def need_pil():
    try:
        from PIL import Image  # noqa: F401
    except ImportError:  # pragma: no cover
        sys.exit("pip install Pillow")


def gbm_to_png_image(g: Gbm, palette: list[int] | None = None):
    """A paletted PNG: the indices are the file's indices, the PLTE is the
    565 palette widened losslessly, and tRNS makes every 0xF81F entry clear."""
    from PIL import Image

    pal = palette if palette is not None else g.palette
    w, h = max(g.width, 1), max(g.height, 1)
    im = Image.new("P", (w, h))
    if g.width and g.height:
        im.putdata(list(g.pixels))
    flat = []
    for c in pal:
        flat += c565_to_rgb(c)
    if not flat:
        flat = [0, 0, 0]
    im.putpalette(flat)
    im.info["transparency"] = bytes(0 if c == TRANSPARENT else 255 for c in pal) or b"\xff"
    return im


def png_bytes(im) -> bytes:
    buf = io.BytesIO()
    im.save(buf, "PNG", transparency=im.info.get("transparency"))
    return buf.getvalue()


def gbm_from_png(im, hint: dict | None, label: str) -> Gbm:
    """Rebuild one GBM. With a paletted PNG the indices are taken as they are
    and each palette entry keeps the manifest's exact 565 value unless the
    PNG's colour for it was edited. With a truecolour PNG every pixel is
    snapped to 565, alpha < 128 becomes the transparent 0xF81F, and the
    palette is the manifest's (when every colour is already in it) or a new
    one in first-seen order."""
    hint = hint or {}
    htype = int(str(hint.get("type", "0x14")), 0)
    hpal = [int(c, 16) for c in hint.get("palette", [])]
    w, h = im.size

    if im.mode == "P":
        idx = bytes(im.getdata())
        plte = im.getpalette() or []
        trns = im.info.get("transparency")
        if isinstance(trns, int):
            trns = bytes(255 if i != trns else 0 for i in range(256))
        n = max(len(hpal), (max(idx) + 1) if idx else 0)
        pal = []
        for i in range(n):
            rgb = tuple(plte[3 * i : 3 * i + 3]) if 3 * i + 3 <= len(plte) else None
            clear = bool(trns) and i < len(trns) and trns[i] == 0
            if i < len(hpal) and (rgb is None or c565_to_rgb(hpal[i]) == rgb) and clear == (hpal[i] == TRANSPARENT):
                pal.append(hpal[i])
            elif clear:
                pal.append(TRANSPARENT)
            elif rgb is not None:
                c = rgb_to_c565(*rgb)
                pal.append(c ^ 0x0001 if c == TRANSPARENT else c)  # opaque magenta-ish, not see-through
            else:
                raise ValueError(f"{label}: index {i} is used but has no palette colour")
        typ = htype
        if len(pal) > 16 and typ & 0xF == 4:
            print(f"  {label}: {len(pal)} colours, promoting 4 bpp -> 8 bpp", file=sys.stderr)
            typ = (typ & 0xF0) | 8
        return Gbm(typ, w, h, pal, idx)

    rgba = im.convert("RGBA")
    cols = []
    for r, g, b, a in rgba.getdata():
        cols.append(TRANSPARENT if a < 128 else rgb_to_c565(r, g, b))
    uniq = list(dict.fromkeys(cols))
    typ = htype
    if hpal and all(c in hpal for c in uniq):
        pal = list(hpal)
    else:
        pal = ([TRANSPARENT] if TRANSPARENT in uniq else []) + [c for c in uniq if c != TRANSPARENT]
        if TRANSPARENT in pal:
            typ |= 0x10  # what the shipped tool sets on images with a clear colour
    limit = 16 if typ & 0xF == 4 else 255
    if len(pal) > limit and typ & 0xF == 4 and len(pal) <= 255:
        print(f"  {label}: {len(pal)} colours, promoting 4 bpp -> 8 bpp", file=sys.stderr)
        typ, limit = (typ & 0xF0) | 8, 255
    if len(pal) > limit:
        sys.exit(f"{label}: {len(pal)} colours after 565 snapping; reduce to {limit} first")
    lut = {c: pal.index(c) for c in pal}
    return Gbm(typ, w, h, pal, bytes(lut[c] for c in cols))


# ----------------------------------------------------------------------------
# commands


def describe(g: Gbm) -> str:
    clear = [i for i, c in enumerate(g.palette) if c == TRANSPARENT]
    return (
        f"type {g.type:#04x} {g.bpp}bpp  {g.width:3d}x{g.height:<3d}  "
        f"{len(g.palette):3d} colours  transparent idx {clear if clear else '-'}"
    )


def cmd_info(a) -> int:
    name, data = load_source(a.src)
    images = mgr_decode(data)
    print(f"{name}: {len(data)} bytes, {len(images)} images")
    for i, g in enumerate(images):
        print(f"  [{i:3d}] {describe(g)}")
    return 0


def cmd_export(a) -> int:
    need_pil()
    from PIL import Image

    name, data = load_source(a.src)
    images = mgr_decode(data)
    remap = load_pal_arg(a.pal)
    out = Path(a.outdir)
    out.mkdir(parents=True, exist_ok=True)
    frames = []
    swatch = Image.new("RGBA", (16 * 12, max(len(images), 1) * 12), (0, 0, 0, 0))
    lines = [f"# {name}  palettes as RGB565 hex, one line per image  * = transparent 0xF81F"]
    for i, g in enumerate(images):
        pal = pal_apply(g.palette, remap) if remap else g.palette
        fn = f"{i:03d}.png"
        (out / fn).write_bytes(png_bytes(gbm_to_png_image(g, pal)))
        frames.append(
            {
                "index": i,
                "png": fn,
                "type": f"{g.type:#04x}",
                "width": g.width,
                "height": g.height,
                "palette": [f"{c:04x}" for c in pal],
            }
        )
        lines.append(
            f"[{i:3d}] {g.width}x{g.height} "
            + " ".join(f"{c:04x}{'*' if c == TRANSPARENT else ''}" for c in pal)
        )
        for k, c in enumerate(pal[:16]):
            col = (255, 0, 255, 0) if c == TRANSPARENT else (*c565_to_rgb(c), 255)
            for yy in range(i * 12, i * 12 + 11):
                for xx in range(k * 12, k * 12 + 11):
                    swatch.putpixel((xx, yy), col)
    manifest = {
        "source": name,
        "palette_remap": a.pal,
        "note": "build re-encodes images in this order; geometry is the PNG's own size; "
        "the image is drawn CENTRED on the point the .cif gives (w/2, h/2 rounded down)",
        "frames": frames,
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=1), encoding="utf-8")
    (out / "palette.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    swatch.save(out / "palette.png")
    print(f"{name}: {len(images)} images -> {out}  (manifest.json, palette.txt, palette.png)")
    return 0


def build_from_manifest(mpath: Path) -> bytes:
    from PIL import Image

    if mpath.is_dir():
        mpath = mpath / "manifest.json"
    m = json.loads(mpath.read_text(encoding="utf-8"))
    images = []
    for fr in m["frames"]:
        png = mpath.parent / fr["png"]
        with Image.open(png) as im:
            im.load()
            images.append(gbm_from_png(im, fr, str(png)))
    return mgr_encode(images)


def cmd_build(a) -> int:
    need_pil()
    data = build_from_manifest(Path(a.manifest))
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    Path(a.out).write_bytes(data)
    images = mgr_decode(data)
    print(f"{a.out}: {len(data)} bytes, {len(images)} images")
    return 0


def contact_sheet(images: list[Gbm], palettes: list[list[int]], title: str, scale: int = 3, maxw: int = 1400):
    from PIL import Image, ImageDraw

    pad, label_h = 6, 12
    cells = []
    for i, g in enumerate(images):
        im = gbm_to_png_image(g, palettes[i]).convert("RGBA")
        im = im.resize((max(g.width, 1) * scale, max(g.height, 1) * scale), Image.NEAREST)
        cells.append((i, g, im))
    # shelf packing
    x = y = pad
    shelf = 0
    pos = []
    width = 0
    for i, g, im in cells:
        cw = max(im.width, 48) + pad
        ch = im.height + label_h + pad
        if x + cw > maxw and x > pad:
            x, y, shelf = pad, y + shelf, 0
        pos.append((x, y))
        x += cw
        width = max(width, x)
        shelf = max(shelf, ch)
    height = y + shelf + pad + 14
    sheet = Image.new("RGBA", (max(width, 200), height), (40, 40, 48, 255))
    d = ImageDraw.Draw(sheet)
    d.text((pad, height - 14), title, fill=(230, 230, 230, 255))
    for (i, g, im), (px, py) in zip(cells, pos):
        # checkerboard under the image
        cb = Image.new("RGBA", im.size, (200, 200, 200, 255))
        cd = ImageDraw.Draw(cb)
        sq = 2 * scale
        for yy in range(0, im.height, sq):
            for xx in range(0, im.width, sq):
                if (xx // sq + yy // sq) & 1:
                    cd.rectangle([xx, yy, xx + sq - 1, yy + sq - 1], fill=(150, 150, 150, 255))
        cb.alpha_composite(im)
        sheet.alpha_composite(cb, (px, py + label_h))
        d.text((px, py), f"{i} {g.width}x{g.height}", fill=(255, 220, 120, 255))
    return sheet


def cmd_sheet(a) -> int:
    need_pil()
    name, data = load_source(a.src)
    images = mgr_decode(data)
    remap = load_pal_arg(a.pal)
    pals = [pal_apply(g.palette, remap) if remap else g.palette for g in images]
    title = f"{name}  {len(images)} images" + (f"  pal {a.pal}" if a.pal else "") + f"  x{a.scale}"
    sheet = contact_sheet(images, pals, title, a.scale)
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    sheet.save(a.out)
    print(f"{name} -> {a.out} ({sheet.width}x{sheet.height})")
    return 0


def cmd_roundtrip(a) -> int:
    """Decode -> paletted PNG bytes -> re-read -> rebuild, for every merger in
    the archive, and demand the exact original bytes."""
    need_pil()
    from PIL import Image

    mgrs = archive().all_mgr()
    ok = fail = images = 0
    for label, data in mgrs:
        try:
            gbms = mgr_decode(data)
            rebuilt = []
            for i, g in enumerate(gbms):
                hint = {
                    "type": f"{g.type:#04x}",
                    "width": g.width,
                    "height": g.height,
                    "palette": [f"{c:04x}" for c in g.palette],
                }
                with Image.open(io.BytesIO(png_bytes(gbm_to_png_image(g)))) as im:
                    im.load()
                    rebuilt.append(gbm_from_png(im, hint, f"{label}[{i}]"))
            images += len(gbms)
            out = mgr_encode(rebuilt)
            direct = mgr_encode(gbms)
            if out == data and direct == data:
                ok += 1
            else:
                fail += 1
                where = next((k for k in range(min(len(out), len(data))) if out[k] != data[k]), None)
                print(f"FAIL {label}: {len(data)} -> {len(out)} bytes, first diff at {where}")
        except Exception as e:  # noqa: BLE001
            fail += 1
            print(f"FAIL {label}: {e}")
    named = sum(1 for lbl, _ in mgrs if not lbl.startswith("0x"))
    print(
        f"roundtrip: {ok}/{len(mgrs)} mergers byte-identical "
        f"({named} named, {len(mgrs) - named} by hash only), {images} images, {fail} failed"
    )
    return 1 if fail else 0


def hue_shift(c: int, deg: float) -> int:
    if c == TRANSPARENT:
        return c
    r, g, b = c565_to_rgb(c)
    h, l, s = colorsys.rgb_to_hls(r / 255, g / 255, b / 255)
    r2, g2, b2 = colorsys.hls_to_rgb((h + deg / 360) % 1.0, l, s)
    out = rgb_to_c565(round(r2 * 255), round(g2 * 255), round(b2 * 255))
    return out ^ 0x0001 if out == TRANSPARENT else out


def cmd_recolor(a) -> int:
    name, data = load_source(a.src)
    images = mgr_decode(data)
    remap = load_pal_arg(a.pal)
    for g in images:
        if remap:
            g.palette = pal_apply(g.palette, remap)
        if a.hue:
            g.palette = [hue_shift(c, a.hue) for c in g.palette]
    out = mgr_encode(images)
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    Path(a.out).write_bytes(out)
    print(f"{name} -> {a.out}: {len(images)} images, {len(out)} bytes (geometry untouched)")
    return 0


def cmd_pal(a) -> int:
    name, data = load_source(a.src)
    pairs = pal_decode(data)
    print(f"{name}: {len(pairs)} remaps")
    for f, t in pairs:
        print(f"  {f:04x} {c565_to_rgb(f)} -> {t:04x} {c565_to_rgb(t)}")
    return 0


# .cif -------------------------------------------------------------------------


@dataclass
class Part:
    layer: int
    index: int
    x: int
    y: int
    flip: int  # (b0 >> 5) & 3: 0 none, 1 H-mirror, 2/3 V-flip
    effect: bytes = b""  # 5-byte tail when b0 & 0x80


@dataclass
class Cif:
    layer_ids: list[int]
    anims: list[list[list[list[Part]]]] = field(default_factory=list)  # [anim][dir][frame] -> parts


def cif_decode(d: bytes, wide: bool) -> Cif:
    """SPRITE_DATA::LoadSprData (0xDBC64) and the reader in SPRITE::DrawSprite
    (0xDA6C8). `wide` is the loader's flag byte: the HERO passes 1 (12-bit
    coordinates), monsters and NPCs 0 (signed bytes)."""

    def s8(v):
        return v - 256 if v > 127 else v

    nl = s8(d[0])
    ids = list(d[1 : 1 + nl])
    r = 1 + nl
    na = d[r]
    r += 1
    cif = Cif(ids)
    for _ in range(na):
        nd = s8(d[r])
        r += 1
        dirs = []
        for _ in range(max(nd, 0)):
            nf = d[r]
            r += 1
            frames = []
            for _ in range(nf):
                np_ = s8(d[r])
                r += 1
                parts = []
                for _ in range(max(np_, 0)):
                    b0 = d[r]
                    if b0 == 0x7F:  # an empty slot the drawer skips
                        r += 5 if wide else 4
                        continue
                    idx = d[r + 1]
                    if wide:
                        hi, mid, lo = d[r + 2], d[r + 3], d[r + 4]
                        x = (hi << 4) | (mid >> 4)
                        y = ((mid & 0xF) << 8) | lo
                        x = x - 0x1000 if x & 0x800 else x
                        y = y - 0x1000 if y & 0x800 else y
                        r += 5
                    else:
                        x, y = s8(d[r + 2]), s8(d[r + 3])
                        r += 4
                    eff = b""
                    if b0 & 0x80:
                        eff = d[r : r + 5]
                        r += 5
                    parts.append(Part(b0 & 0x1F, idx, x, y, (b0 >> 5) & 3, eff))
                frames.append(parts)
            dirs.append(frames)
        cif.anims.append(dirs)
    if r != len(d):
        raise ValueError(f"cif parse ended at {r} of {len(d)} -- wrong `wide` flag?")
    return cif


def cmd_cif(a) -> int:
    name, data = load_source(a.src)
    try:
        cif = cif_decode(data, wide=not a.narrow)
    except (ValueError, IndexError):
        cif = cif_decode(data, wide=a.narrow)
        a.narrow = not a.narrow
    print(
        f"{name}: {len(cif.layer_ids)} layers (authoring ids {cif.layer_ids}), "
        f"{len(cif.anims)} animations, {'8-bit' if a.narrow else '12-bit'} coordinates"
    )
    uses: dict[int, dict[int, list]] = {}
    for ai, dirs in enumerate(cif.anims):
        for di, frames in enumerate(dirs):
            for fi, parts in enumerate(frames):
                for p in parts:
                    uses.setdefault(p.layer, {}).setdefault(p.index, []).append((ai, di, fi, p))
    for layer in sorted(uses):
        idx = uses[layer]
        print(
            f"  layer {layer} (_szSprFile[{layer}]): {len(idx)} distinct images, "
            f"max index {max(idx)}  -> the sheet needs >= {max(idx) + 1} images"
        )
        if a.layer is not None and a.layer == layer:
            for i in sorted(idx):
                us = idx[i]
                xs = sorted({(p.x, p.y, p.flip) for *_, p in us})
                print(f"    [{i:3d}] {len(us):4d} uses, centres (x,y,flip): {xs[:8]}{' ...' if len(xs) > 8 else ''}")
    if a.anims:
        for ai, dirs in enumerate(cif.anims):
            print(f"  anim {ai:2d}: {len(dirs)} dir(s), frames {[len(f) for f in dirs]}")
    return 0


CLASS_WEAPON = {0: 0, 1: 16, 2: 35, 3: 50}  # SetLoadImgPath's no-weapon defaults (arg != 0)


def cmd_pose(a) -> int:
    """Assemble one animation the way SPRITE::DrawSprite does: parts in list
    order, each CENTRED at origin + (x, y); LEFT is RIGHT mirrored (x -> -x,
    flip toggled) like the guest does for 3-direction files. Plain parts are
    exact. Effect-tail parts (b0 & 0x80) are approximated: 90/180/270 rotation
    (sense not verified against the blitter), scale and opacity."""
    need_pil()
    from PIL import Image, ImageDraw

    name, data = load_source(a.cif)
    cif = cif_decode(data, wide=True)
    stem = Path(name).stem
    cls = int(stem) - 1 if stem.isdigit() and 1 <= int(stem) <= 4 else 0
    layers = {
        0: "c/sp/img0/000.mgr",
        1: "c/sp/img1/000.mgr",
        2: "c/sp/img2/000.mgr",
        3: "c/sp/img3/000.mgr",
        4: f"c/sp/img4/{CLASS_WEAPON[cls]:03d}.mgr",
        5: f"c/sp/img5/{cls:03d}.mgr",
        6: "c/sp/img6/000.mgr" if cls == 3 else "c/sp/empty/empty.mgr",
    }
    pals: dict[int, list] = {}
    for s in a.set or []:
        k, v = s.split("=", 1)
        layers[int(k)] = v
    for s in a.pal or []:
        k, v = s.split("=", 1)
        pals[int(k)] = load_pal_arg(v)
    sheets = {}
    for k, src in layers.items():
        if k >= len(cif.layer_ids):
            continue
        imgs = mgr_decode(load_source(src)[1])
        sheets[k] = [
            gbm_to_png_image(g, pal_apply(g.palette, pals[k]) if k in pals else None).convert("RGBA") for g in imgs
        ]
    if a.anim >= len(cif.anims):
        sys.exit(f"{name} has {len(cif.anims)} animations")
    dirs = cif.anims[a.anim]
    # direction rows the way DrawSprite maps them: 0 up, 1 down, 2 right, 3 left
    rows = []
    for want in range(4):
        n = len(dirs)
        mirror = False
        d = want
        if n == 1:
            d, mirror = 0, want == 3
        elif n == 2:
            d, mirror = (1, want == 3) if want in (1, 2, 3) else (0, False)
        elif n == 3 and want == 3:
            d, mirror = 2, True
        if d < n:
            rows.append((want, d, mirror))
    cell, ox, oy = 96, 48, 72
    ncol = max(len(dirs[d]) for _, d, _ in rows)
    out = Image.new("RGBA", (ncol * cell, len(rows) * cell), (88, 104, 88, 255))
    missing = set()
    for r, (want, d, mirror) in enumerate(rows):
        for f, parts in enumerate(dirs[d]):
            for p in parts:
                imgs = sheets.get(p.layer)
                if not imgs:
                    continue
                if p.index >= len(imgs):
                    missing.add((p.layer, p.index))
                    continue
                im = imgs[p.index]
                flip, x = p.flip, p.x
                if mirror:
                    x = -x
                    flip = {0: 1, 1: 0}.get(flip, 3)
                if p.effect:
                    sx, sy, rot, alpha, _ = p.effect
                    rot *= 2
                    if mirror and rot:
                        rot = 360 - rot
                    if (sx, sy) != (25, 25):
                        im = im.resize((max(1, im.width * sx * 4 // 100), max(1, im.height * sy * 4 // 100)), Image.NEAREST)
                    if rot:
                        im = im.rotate(-rot if a.cw else rot, expand=True)
                    if alpha < 255:
                        im = im.copy()
                        im.putalpha(im.getchannel("A").point(lambda v, k=alpha: v * k // 255))
                if flip == 1:
                    im = im.transpose(Image.FLIP_LEFT_RIGHT)
                elif flip == 2:
                    im = im.transpose(Image.FLIP_TOP_BOTTOM)
                elif flip == 3:
                    im = im.transpose(Image.ROTATE_180)
                out.alpha_composite(im, (f * cell + ox + x - im.width // 2, r * cell + oy + p.y - im.height // 2))
    if missing:
        print(f"  !! indices past the end of their sheet (the guest would read heap garbage): {sorted(missing)}")
    big = out.resize((out.width * a.scale, out.height * a.scale), Image.NEAREST)
    d = ImageDraw.Draw(big)
    for r, (want, _, mirror) in enumerate(rows):
        d.text((2, r * cell * a.scale + 2), ["up", "down", "right", "left"][want] + (" (mirrored)" if mirror else ""), fill=(255, 230, 140, 255))
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    big.save(a.out)
    print(f"{name} anim {a.anim}: {len(rows)} directions x {ncol} frames -> {a.out}")
    return 0


# listing ---------------------------------------------------------------------


def ranges(nums: list[int]) -> str:
    out, start, prev = [], None, None
    for n in nums:
        if start is None:
            start = prev = n
        elif n == prev + 1:
            prev = n
        else:
            out.append(f"{start:03d}" if start == prev else f"{start:03d}-{prev:03d}")
            start = prev = n
    if start is not None:
        out.append(f"{start:03d}" if start == prev else f"{start:03d}-{prev:03d}")
    return ", ".join(out)


def cmd_free(a) -> int:
    ar = archive()
    for d in range(10):
        used = [n for n in range(1000) if vfs.djb2(f"c/sp/img{d}/{n:03d}.mgr") in ar.by_hash]
        if not used:
            continue
        free = [n for n in range(max(used) + 1) if n not in used]
        print(f"/c/sp/img{d}/: {len(used)} sheets, highest {max(used):03d}")
        print(f"    used: {ranges(used)}")
        print(f"    free below the highest: {ranges(free) or 'none'};  free: {max(used) + 1:03d}-999")
    return 0


def cmd_ls(a) -> int:
    mgrs = archive().all_mgr()
    for label, data in mgrs:
        n = struct.unpack_from("<I", data, 0)[0]
        print(f"{label:28s} {n:4d} images {len(data):8d} bytes")
    print(f"-- {len(mgrs)} mergers", file=sys.stderr)
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("info", help="list the images in a sheet")
    p.add_argument("src")
    p.set_defaults(fn=cmd_info)

    p = sub.add_parser("export", help="one paletted PNG per image + manifest.json + palette dump")
    p.add_argument("src")
    p.add_argument("outdir")
    p.add_argument("--pal", help="apply a recolour table first: a number (c/sp/pal/NNN.pal) or a path")
    p.set_defaults(fn=cmd_export)

    p = sub.add_parser("build", help="re-encode a sheet from manifest.json and its PNGs")
    p.add_argument("manifest", help="manifest.json or the directory holding it")
    p.add_argument("out")
    p.set_defaults(fn=cmd_build)

    p = sub.add_parser("sheet", help="contact sheet of every image, scaled, on a checkerboard")
    p.add_argument("src")
    p.add_argument("out")
    p.add_argument("--pal")
    p.add_argument("--scale", type=int, default=3)
    p.set_defaults(fn=cmd_sheet)

    p = sub.add_parser("roundtrip", help="decode+re-encode every .mgr in the archive, demand identical bytes")
    p.set_defaults(fn=cmd_roundtrip)

    p = sub.add_parser("recolor", help="write a recoloured copy (geometry untouched)")
    p.add_argument("src")
    p.add_argument("out")
    p.add_argument("--hue", type=float, default=0.0, help="rotate every opaque palette colour by DEG")
    p.add_argument("--pal", help="bake a .pal recolour table into the palettes")
    p.set_defaults(fn=cmd_recolor)

    p = sub.add_parser("pal", help="print a .pal recolour table")
    p.add_argument("src")
    p.set_defaults(fn=cmd_pal)

    p = sub.add_parser("cif", help="which images of each layer an animation file uses")
    p.add_argument("src")
    p.add_argument("--layer", type=int, help="also list every centre offset used for this layer")
    p.add_argument("--anims", action="store_true", help="also list animations and frame counts")
    p.add_argument("--narrow", action="store_true", help="8-bit coordinates (monster/NPC files)")
    p.set_defaults(fn=cmd_cif)

    p = sub.add_parser("pose", help="preview an animation assembled from layer sheets (costume check)")
    p.add_argument("cif", help="c/sp/cif/001.cif (warrior) .. 004.cif (knight)")
    p.add_argument("out")
    p.add_argument("--anim", type=int, default=0, help="animation number (0 = idle)")
    p.add_argument("--set", action="append", metavar="LAYER=SRC", help="e.g. 1=costume.mgr or 1=c/sp/img1/051.mgr")
    p.add_argument("--pal", action="append", metavar="LAYER=PAL", help="e.g. 1=79 applies c/sp/pal/079.pal")
    p.add_argument("--scale", type=int, default=3)
    p.add_argument("--cw", action="store_true", help="rotate effect parts clockwise instead")
    p.set_defaults(fn=cmd_pose)

    p = sub.add_parser("free", help="used and free numbers in every /c/sp/imgN/")
    p.set_defaults(fn=cmd_free)

    p = sub.add_parser("ls", help="every merger in the archive")
    p.set_defaults(fn=cmd_ls)

    a = ap.parse_args()
    return a.fn(a)


if __name__ == "__main__":
    raise SystemExit(main())
