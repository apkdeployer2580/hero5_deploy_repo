//! Host reimplementations of guest functions, selected by name.
//!
//! The interpreter is the whole cost of a frame: on a Galaxy S9 the guest
//! retires ~33 M instructions a second and a frame needs ~3.4 M of them, which
//! is ~10 fps however fast the presenting is. `hl5_probe --profile` says where
//! they go, and the answer has never been spread out — it has been one or two
//! functions at a time, each in turn:
//!
//! ```text
//!   before any port      after MC_grpFillRect      after the blending blitter
//!     66.7% grpFillRect    70.5% BlendingClip565      15.8% BlendingClip565 (4-bit only)
//!     21.1% BlendingClip    5.2% MirrorClip565        14.9% MirrorClip565
//!                           5.1% FillRectBlending     14.5% FillRectBlending
//!                           4.4% FntDrawString        12.5% FntDrawString
//!
//!   new-game replay, guest instructions:
//!     830.2 M  ->  290.3 M      (2.86x, 119/119 frames byte-identical)
//! ```
//!
//! The list above was measured on the *new-game* replay, and that turned out to
//! be a trap: it never gets far enough in to draw much. Profiling the **ingame**
//! replay instead found one function worth all the others put together —
//!
//! ```text
//!   ingame replay, before                after copy_frame_buffer
//!     81.1% internalGrpCopyFrameBuffer     25.6% BlendingClip565 (4-bit only)
//!      4.8% BlendingClip565                12.6% FntDrawString
//!      2.4% FntDrawString                  11.7% StretchClip565
//!      2.2% StretchClip565                 10.6% MirrorClip565
//!      2.0% MirrorClip565                   8.0% ChangeFrameBufferBlending565
//!                                           6.5% FillRectBlending
//!
//!   ingame replay, guest instructions:
//!     25.30 G (--no-patch)  ->  20.29 G  ->  3.84 G     (6.58x, 543/543 identical)
//!     shop 9.56x, online 9.90x, newgame unchanged (it never calls it)
//! ```
//!
//! — and the profile is flat now, which is the point at which the remaining
//! entries are worth doing in order rather than guessing.
//!
//! So the lever is to run those in native code and leave everything else
//! interpreted. This module is that lever: a map from a guest **symbol** to a
//! host function, installed at load time. When the interpreter is about to
//! execute the first instruction of a patched function it calls the host body
//! instead, puts the result in `r0`, and returns to `lr`.
//!
//! # The rule these have to obey
//!
//! A port is only allowed if it produces the **same bytes**. Not "looks the
//! same" — the same bytes. A blend rounded the other way or a clip off by one
//! looks fine and is wrong, and surfaces months later as one corrupt sprite in
//! one scene nobody replayed. So each port here is a transcription of the
//! disassembly, with the addresses of what it was transcribed from, and the
//! acceptance test is two runs of the same deterministic replay — one patched,
//! one not — whose dumped frames are byte-identical:
//!
//! ```text
//!   cargo run -p hl5_probe -- --boot --script tools_re/scripts/newgame.txt \
//!       --film out/film_ref --no-patch
//!   cargo run -p hl5_probe -- --boot --script tools_re/scripts/newgame.txt \
//!       --film out/film_new
//!   python tools_re/ab_compare.py out/film_ref out/film_new
//! ```

use crate::Vm;
use arm32_cpu::{reg, Mode};

/// A host stand-in for one guest function. It reads its arguments from the
/// CPU, does the work, and leaves the return value in `r0`; the caller-side
/// plumbing (returning to `lr`) is [`Vm::run`]'s.
///
/// Returning `false` **declines** the call: the interpreter runs the original
/// code instead, exactly as if there were no patch. That is not a failure
/// path, it is the design. The blitters here are single functions containing
/// a dozen specialised inner loops — colour depth, blend mode, direction — and
/// a game exercises two or three of them. Porting only those, and declining
/// the rest, is correct by construction: an unported variant runs the shipped
/// code. The alternative — porting all of them to avoid a branch — is how a
/// rarely-taken variant ends up subtly wrong in a scene nobody replayed.
pub type PatchFn = fn(&mut Vm) -> Result<bool, String>;

/// Guest functions this module can serve, by symbol name.
///
/// Names, never addresses: the binary kept its symbol table, so a patch that
/// no longer matches shows up as "no such symbol" at load rather than as a
/// jump into the middle of something else.
pub const PATCHES: &[(&str, PatchFn)] = &[
    ("MC_grpFillRect", fill_rect),
    (
        "_Z26internalGrpCopyFrameBufferiiiiiiiiP14_MC_GrpContexth",
        copy_frame_buffer,
    ),
    ("_ZN7Graphic13FillRectAlphaEiiiiihi", fill_rect_alpha),
    (
        "_ZN7Graphic24DrawImageBlendingClip565EiiiiiP8GbmImageiihhhi",
        draw_image_blending_clip565,
    ),
    (
        "_ZN7Graphic28ChangeFrameBufferBlending565Eiihh",
        change_frame_buffer_blending565,
    ),
    ("_ZN7Graphic16FillRectBlendingEiiiiihhi", fill_rect_blending),
    (
        "_ZN7Graphic23DrawImageStretchClip565EiiiiiP8GbmImageiihiihhi",
        draw_image_stretch_clip565,
    ),
    (
        "_ZN7Graphic24DrawImageStretchClip565REiiiiiP8GbmImageiihiihhi",
        draw_image_stretch_clip565r,
    ),
    (
        "_ZN7Graphic22DrawImageMirrorClip565EiiiiiP8GbmImageiihi",
        draw_image_mirror_clip565,
    ),
    ("_midas_funcFntDrawPixel", fnt_draw_pixel),
    // The two one-frame stalls: opening a menu (the blur behind it) and
    // saving (the DES over every save file). Not frame-budget items like the
    // blitters above — each runs once per button press, and that once was
    // ~0.9 s / ~1.5 s of frozen screen on a Galaxy S9 (ANALYSIS.md §55).
    ("_ZN7Graphic10BoxBlur565Eii", box_blur565),
    ("startDes", crate::des::start_des),
];

/// `MC_GrpContext`, at the offsets the guest actually touches.
///
/// The field *names* come from the IDA database that shipped inside the APK
/// (`tools_re/idb_structs.txt`), which recovered the order but no offsets. The
/// offsets below are from the access code in `MC_grpFillRect`, and they line
/// that list up exactly: mask, clip[4], fgpxl, bgpxl, transpxl, alpha,
/// offsetX, offsetY, pfnPixelOp, param1.
pub(crate) mod ctx {
    /// Bit 0 set means the clip rectangle below is live; clear means
    /// unbounded. `MC_grpSetContext` only ever ORs this, never clears it, so
    /// once any screen has set a clip the flag stays set for the rest of the
    /// run.
    pub const MASK: u32 = 0x00;
    pub const CLIP_X0: u32 = 0x04;
    pub const CLIP_Y0: u32 = 0x08;
    pub const CLIP_X1: u32 = 0x0c;
    pub const CLIP_Y1: u32 = 0x10;
    pub const FGPXL: u32 = 0x14;
    /// The colour treated as "do not draw" when the caller asks for it.
    /// `internalGrpCopyFrameBuffer` reads it at 0x148714.
    pub const TRANSPXL: u32 = 0x1c;
    pub const ALPHA: u32 = 0x20;
    pub const PFN_PIXEL_OP: u32 = 0x2c;
    pub const PARAM1: u32 = 0x30;
}

/// The frame buffer descriptor, from the same function's loads.
///
/// `pub(crate)` because `shop.rs`'s price hook draws its strikethrough into
/// the same descriptor — the layout is one fact, read from one place.
pub(crate) mod fb {
    pub const WIDTH: u32 = 0x00;
    pub const HEIGHT: u32 = 0x04;
    /// Bytes per row.
    pub const STRIDE: u32 = 0x08;
    /// Bits per pixel: 0x10 for RGB565, 0x20 for 32-bit.
    ///
    /// This one is worth spelling out because it is the trap in this
    /// function. `MC_grpFillRect` opens by calling
    /// `MIDASKernelManager::getInstance()` and then `getKernel()`, which reads
    /// like "ask the kernel what the pixel format is" — and it is dead code.
    /// `getKernel` is a bare `bx lr`, its result is dropped, and the value
    /// actually used comes from `ldr fp, [r1, #0xc]` with `r1` reloaded from
    /// the *frame buffer* argument. Reading the kernel instead draws nothing
    /// at all, because neither 0x10 nor 0x20 matches whatever is there.
    pub const BPP: u32 = 0x0c;
    /// Pixel storage.
    pub const BASE: u32 = 0x10;
}

/// `_midas_funcGrpPixelOpProc` (0x148024) — the RGB565 source-over blend.
///
/// Transcribed field by field. Note it ignores its `bpp` argument: the 32-bit
/// path calls it too and gets 565 arithmetic, which is what the shipped code
/// does and therefore what this has to do.
fn blend565(dst: u32, src: u32, alpha: u32) -> u32 {
    let ia = 0xff - (alpha & 0xff);
    let a = alpha & 0xff;
    let g = (src & 0x07e0) * a + (dst & 0x07e0) * ia;
    let r = (src & 0xf800) * a + (dst & 0xf800) * ia;
    let b = (src & 0x001f) * a + (dst & 0x001f) * ia;
    ((g >> 8) & 0x07e0) | ((r >> 8) & 0xf800) | ((b >> 8) & 0x001f)
}

/// `MC_grpFillRect` (0x1497ac) — 66.7% of this game's guest time.
///
/// `(fb, x, y, w, h, ctx)`, with `h` and `ctx` on the stack. Negative extents
/// are normalised, then the rect is clamped to the buffer, then every pixel is
/// tested against the context's clip rectangle before it is written — that
/// per-pixel clip test is what makes the interpreted version so expensive, and
/// it is also why the loop cannot simply be hoisted into a row fill without
/// changing behaviour when `m_pfnPixelOp` is set.
/// `internalGrpCopyFrameBuffer` (0x148430) — the surface-to-surface blit under
/// `MC_grpCopyFrameBuffer`, and by a wide margin the most expensive thing the
/// guest does: **81% of all retired instructions** in the ingame replay
/// (`--profile`), with the next entry at 4.8%.
///
/// The cost is not the copying, it is the per-pixel preamble. The shipped
/// inner loop (0x148528..0x1485f4) performs twelve bounds and clip tests —
/// each a load and a compare — before it moves one halfword. That is ~25
/// interpreted instructions of overhead per pixel, over a full surface, every
/// frame; `--profile-in` puts 58% of the function at +0x0c0..+0x1c0, which is
/// exactly that chain, and 40% in the 16-bit body at +0x280..+0x390.
///
/// Every one of those tests is either invariant across the inner loop or is
/// "is x inside a half-open range", so this evaluates them once per row and
/// then copies the surviving run. The arithmetic is the shipped arithmetic,
/// including its quirks — see the `src_y` note below.
///
/// Ported: 16-bit destination, opaque, no pixel op — what the game uses to
/// present. Declined, and left to the interpreter:
///
/// * a destination that is not 0x10 bpp (the 0x20 arm at 0x148620),
/// * `m_alpha <= 0xfe`, which calls `_midas_funcGrpPixelOpProc` (0x148024)
///   once per pixel (0x14874c),
/// * a non-null `m_pfnPixelOp`, which is an indirect guest call per pixel
///   (0x1487a4).
fn copy_frame_buffer(vm: &mut Vm) -> Result<bool, String> {
    let dst = vm.arg(0);
    let dst_x = vm.arg(1) as i32;
    let dst_y = vm.arg(2) as i32;
    let mut w = vm.arg(3) as i32;
    let mut h = vm.arg(4) as i32;
    let src = vm.arg(5);
    let mut src_x = vm.arg(6) as i32;
    let mut src_y = vm.arg(7) as i32;
    let cx = vm.arg(8);
    let keyed = vm.arg(9) & 0xff;

    // Decline first, before reading anything else and before any store: the
    // interpreter has to be able to run the shipped body from a clean slate.
    let bpp = vm.mem.read_u32(dst + fb::BPP);
    let alpha = vm.mem.read_u32(cx + ctx::ALPHA);
    let pfn = vm.mem.read_u32(cx + ctx::PFN_PIXEL_OP);
    if bpp != 0x10 || alpha <= 0xfe || pfn != 0 {
        return Ok(false);
    }

    let dst_w = vm.mem.read_u32(dst + fb::WIDTH) as i32;
    let dst_h = vm.mem.read_u32(dst + fb::HEIGHT) as i32;
    let dst_stride = vm.mem.read_u32(dst + fb::STRIDE) as i32;
    let dst_base = vm.mem.read_u32(dst + fb::BASE);
    let src_w = vm.mem.read_u32(src + fb::WIDTH) as i32;
    let src_h = vm.mem.read_u32(src + fb::HEIGHT) as i32;
    let src_stride = vm.mem.read_u32(src + fb::STRIDE) as i32;
    let src_base = vm.mem.read_u32(src + fb::BASE);
    let transpxl = vm.mem.read_u32(cx + ctx::TRANSPXL);

    // Entry clipping, 0x148438..0x1484cc, in the order the original does it.
    if src_x < 0 {
        w += src_x;
        src_x = 0;
    }
    if src_y < 0 {
        // Transcribed, not corrected: 0x148490 is `addlt fp, fp, sb`, and `fp`
        // is the *width*. A negative source Y shortens the row rather than the
        // column count. Nothing in the replays reaches it with src_y < 0, so
        // this has never been observed to matter — but a port that "fixed" it
        // would stop being a port.
        w += src_y;
        src_y = 0;
    }
    if w + dst_x > dst_w {
        w = dst_w - dst_x;
    }
    if h + dst_y > dst_h {
        h = dst_h - dst_y;
    }
    if h <= 0 {
        return Ok(true);
    }

    // Row-invariant tests, hoisted out of the inner loop (0x148528, 0x148548,
    // 0x14856c, 0x14857c and the clip Y pair at 0x1485a0/0x1485b8).
    let clip_x0 = vm.mem.read_u32(cx + ctx::CLIP_X0) as i32;
    let clip_y0 = vm.mem.read_u32(cx + ctx::CLIP_Y0) as i32;
    let clip_x1 = vm.mem.read_u32(cx + ctx::CLIP_X1) as i32;
    let clip_y1 = vm.mem.read_u32(cx + ctx::CLIP_Y1) as i32;

    // Column tests reduce to one half-open range: x must keep both
    // `src_x + x` inside the source and `dst_x + x` inside the destination
    // *and* the clip box.
    let dx_lo = clip_x0.max(0);
    let dx_hi = dst_w.min(clip_x1);
    let x0 = 0.max(-src_x).max(dx_lo - dst_x);
    let x1 = w.min(src_w - src_x).min(dx_hi - dst_x);
    if x0 >= x1 {
        return Ok(true);
    }

    for row in 0..h {
        let sy = src_y + row;
        let dy = dst_y + row;
        if sy < 0 || sy >= src_h || dy < 0 || dy >= dst_h {
            continue;
        }
        if dy < clip_y0 || dy >= clip_y1 {
            continue;
        }
        // 0x1486fc / 0x148738: both bases are byte strides.
        let src_row = src_base.wrapping_add((sy * src_stride) as u32);
        let dst_row = dst_base.wrapping_add((dy * dst_stride) as u32);
        for x in x0..x1 {
            let sp = src_row.wrapping_add(((src_x + x) * 2) as u32);
            let px = vm.mem.read_u16(sp) as u32;
            // 0x148700..0x14871c: the colour-key test happens only when the
            // caller passed a non-zero flag, and compares before any write.
            if keyed != 0 && px == transpxl {
                continue;
            }
            let dp = dst_row.wrapping_add(((dst_x + x) * 2) as u32);
            vm.mem.write_u16(dp, (px & 0xffff) as u16);
        }
    }
    Ok(true)
}

fn fill_rect(vm: &mut Vm) -> Result<bool, String> {
    let fbp = vm.arg(0);
    let mut x = vm.arg(1) as i32;
    let mut y = vm.arg(2) as i32;
    let mut w = vm.arg(3) as i32;
    let mut h = vm.arg(4) as i32;
    let cx = vm.arg(5);

    let bpp = vm.mem.read_u32(fbp + fb::BPP);

    if w < 0 {
        x += w;
        w = -w;
    }
    if h < 0 {
        y += h;
        h = -h;
    }
    if x < 0 {
        w += x;
        x = 0;
    }
    if y < 0 {
        h += y;
        y = 0;
    }

    let buf_w = vm.mem.read_u32(fbp + fb::WIDTH) as i32;
    let buf_h = vm.mem.read_u32(fbp + fb::HEIGHT) as i32;
    let stride = vm.mem.read_u32(fbp + fb::STRIDE) as i32;
    let base = vm.mem.read_u32(fbp + fb::BASE);

    let y_end = (y + h).min(buf_h);
    let x_end = (x + w).min(buf_w);

    let clip_x0 = vm.mem.read_u32(cx + ctx::CLIP_X0) as i32;
    let clip_y0 = vm.mem.read_u32(cx + ctx::CLIP_Y0) as i32;
    let clip_x1 = vm.mem.read_u32(cx + ctx::CLIP_X1) as i32;
    let clip_y1 = vm.mem.read_u32(cx + ctx::CLIP_Y1) as i32;
    let fg = vm.mem.read_u32(cx + ctx::FGPXL);
    let alpha = vm.mem.read_u32(cx + ctx::ALPHA);
    let pfn = vm.mem.read_u32(cx + ctx::PFN_PIXEL_OP);
    let param1 = vm.mem.read_u32(cx + ctx::PARAM1);

    // The rows outside the clip cost nothing but the test; skipping them up
    // front is not a behaviour change because the body has no side effects.
    let y0 = y.max(clip_y0);
    let y1 = y_end.min(clip_y1);
    let x0 = x.max(clip_x0);
    let x1 = x_end.min(clip_x1);
    if y0 >= y1 || x0 >= x1 {
        return Ok(true);
    }

    let opaque = alpha > 0xfe;
    for yy in y0..y1 {
        let row = base.wrapping_add((yy * stride) as u32);
        for xx in x0..x1 {
            if bpp == 0x10 {
                let p = row.wrapping_add((xx * 2) as u32);
                let mut col = if opaque {
                    fg
                } else {
                    blend565(vm.mem.read_u16(p) as u32, fg, alpha)
                };
                if pfn != 0 {
                    col = vm.call_pixel_op(pfn, vm.mem.read_u16(p) as u32, col, param1);
                }
                vm.mem.write_u16(p, col as u16);
            } else if bpp == 0x20 {
                let p = row.wrapping_add((xx * 4) as u32);
                let mut col = if opaque {
                    fg
                } else {
                    blend565(vm.mem.read_u32(p), fg, alpha)
                };
                if pfn != 0 {
                    col = vm.call_pixel_op(pfn, vm.mem.read_u32(p), col, param1);
                }
                vm.mem.write_u32(p, col);
            }
        }
    }
    Ok(true)
}

/// `FrameBufferInfo` (0x51FE0 / 0x52048) — a local snapshot of a frame buffer
/// descriptor, with **no side effects**: the constructor copies six fields and
/// the destructor zeroes them. That is why the ports below can read the
/// descriptor directly instead of re-entering the guest to build one.
struct FbInfo {
    /// Bytes per line (`GetBpl`).
    bpl: i32,
    width: i32,
    height: i32,
    /// Bytes per pixel (`GetRbgBufSize`) — the constructor computes it as
    /// `bpl / width`.
    bpp_bytes: i32,
    base: u32,
}

impl FbInfo {
    fn read(vm: &Vm, src: u32) -> Self {
        let bpl = vm.mem.read_u32(src + 0x08) as i32;
        let width = vm.mem.read_u32(src + 0x00) as i32;
        FbInfo {
            bpl,
            width,
            height: vm.mem.read_u32(src + 0x04) as i32,
            bpp_bytes: if width != 0 { bpl / width } else { 0 },
            base: vm.mem.read_u32(src + 0x10),
        }
    }
}

/// `Graphic::FillRectAlpha` (0x52224) — 10.2% of guest time once
/// `MC_grpFillRect` is out of the way.
///
/// `(fb, x, y, w, h, alpha, rgb)`. The colour arrives as 24-bit `0xRRGGBB` and
/// is pre-multiplied by alpha once, outside the loops; the per-pixel work is
/// `dst * (255 - alpha) + premultiplied`, done in the packed layout rather than
/// unpacked to bytes.
///
/// Two variants, chosen by `this[0x5C]`: 1 is RGB565 and 2 is the 666 layout.
/// Anything else falls out of the shipped function without drawing, so this
/// declines and lets the interpreter reproduce that exactly.
fn fill_rect_alpha(vm: &mut Vm) -> Result<bool, String> {
    let this = vm.arg(0);
    let fbp = vm.arg(1);
    let mut x = vm.arg(2) as i32;
    let mut y = vm.arg(3) as i32;
    let mut w = vm.arg(4) as i32;
    let mut h = vm.arg(5) as i32;
    let alpha = vm.arg(6) & 0xff;
    let rgb = vm.arg(7);

    // The shipped function returns immediately on a zero alpha, before it even
    // builds the FrameBufferInfo.
    if alpha == 0 {
        return Ok(true);
    }
    // `this[0x5C]` selects the pixel layout: 1 is RGB565, 2 is the 666 one,
    // and anything else falls out of the shipped function without drawing.
    //
    // Only 565 is ported. The 666 branch was transcribed first and then
    // removed, because measuring said it is never taken: the display is
    // initialised as 5/6/5 (`nativeInitDisplay`), and declining it changed
    // neither the instruction count nor a single pixel across the whole
    // replay. Keeping an unexercised transcription would have meant shipping a
    // path the A/B cannot vouch for — the interpreter runs it correctly, and
    // it costs nothing measurable.
    let mode = vm.mem.read_u16(this + 0x5c) as u32 & 0xff;
    if mode != 1 {
        return Ok(false);
    }

    let fb = FbInfo::read(vm, fbp);

    // Negative origins pull the extent in and clamp the origin to zero — the
    // same shape as MC_grpFillRect, but here only the origin is normalised.
    if x < 0 {
        w += x;
        x = 0;
    }
    if y < 0 {
        h += y;
        y = 0;
    }
    if x + w > fb.width {
        w = fb.width - x;
    }
    if y + h > fb.height {
        h = fb.height - y;
    }
    if h <= 0 {
        return Ok(true);
    }

    let ia = 255u32.wrapping_sub(alpha);
    // Pre-multiplied colour, exactly as the guest computes it: the components
    // are taken from the 24-bit value and scaled by alpha once.
    let pr = ((rgb & 0x00ff_0000) >> 16) * alpha;
    let pg = ((rgb & 0x0000_ff00) >> 8) * alpha;
    let pb = (rgb & 0x0000_00ff) * alpha;

    let mut row = fb
        .base
        .wrapping_add((y * fb.bpl) as u32)
        .wrapping_add((x * fb.bpp_bytes) as u32);

    for _ in 0..h {
        if w > 0 {
            for i in 0..w {
                // RGB565. Transcribed field by field from 0x52394; the
                // shifts look asymmetric because the >>8 that would normalise
                // each component and the <<11/<<5 that would repack it are
                // folded together.
                let p = row.wrapping_add((i * 2) as u32);
                let px = vm.mem.read_u16(p) as u32;
                let g = ((px & 0x07e0) >> 3).wrapping_mul(ia).wrapping_add(pg);
                let r = ((px & 0xf800) >> 8).wrapping_mul(ia).wrapping_add(pr);
                let b = ((px << 3) & 0xff).wrapping_mul(ia).wrapping_add(pb);
                let out = (r & 0xf800) | ((g >> 5) & 0x07e0) | ((b << 16) >> 27);
                vm.mem.write_u16(p, out as u16);
            }
        }
        row = row.wrapping_add(fb.bpl as u32);
    }
    Ok(true)
}

/// `GbmImage` (0x8529C..0x852DC) — the accessors are two instructions each, so
/// the offsets are read directly rather than called.
mod gbm {
    /// Low nibble is the colour depth selector: 4 means a 4-bit palette.
    pub const COLOR_BIT: u32 = 0x00;
    pub const WIDTH: u32 = 0x02;
    pub const HEIGHT: u32 = 0x04;
    /// RGB565 palette, one half-word per entry.
    pub const PALETTE: u32 = 0x08;
    pub const PIXELS: u32 = 0x0c;
}

/// The transform selector, argument 9 of the image blitters.
///
/// All four values mean the same thing wherever they appear: the source always
/// scans forward and the *destination* corner and step signs change.
/// [`draw_image_mirror_clip565`] proves that, because it has one arm per value
/// at each colour depth and they differ in nothing else.
/// [`draw_image_blending_clip565`] ports all but `MIRROR_Y`; its own census
/// says why.
///
/// The two dispatchers that read it are +0x16C (8-bit) and +0x1B8 (4-bit), and
/// they test the same four values in the same order — 0x10, 0x08, 0x04, 0x0c —
/// so the pairing below is one fact per depth, not two.
mod xform {
    /// Straight copy — source and destination both scan forward.
    pub const NORMAL: u32 = 0x10;
    /// Mirror in x: the source scans forward, the destination walks left from
    /// the **top**-right corner and the rows still run top to bottom.
    ///
    /// The only thing separating this from [`ROT180`] is the sign of the
    /// per-row step: +0x1458 `add fp,fp,r8` (`w + GetWidth()`) here against
    /// +0x264 `rsb fp,fp,r8` (`w - GetWidth()`) there, and a start index of
    /// `y*GetWidth()` against `(y+h-1)*GetWidth()`. Everything between those
    /// two instructions is the same code twice.
    pub const MIRROR_X: u32 = 0x08;
    /// Mirrored top-to-bottom: the destination scans forward from the
    /// bottom-left corner, a row at a time upwards.
    ///
    /// `draw_image_blending_clip565` declines this one (its census says the
    /// 8-bit body is entered 0 times and the 4-bit body 164 times against
    /// 3.6 M calls); `draw_image_mirror_clip565` ports it.
    pub const MIRROR_Y: u32 = 0x04;
    /// 180 degrees: the source still scans forward, the destination walks
    /// backwards from the bottom-right corner.
    pub const ROT180: u32 = 0x0c;
}

/// Scratch for the out-parameters of the two guest helpers this port calls
/// back into, carved just below the caller's frame.
///
/// It has to sit **above** the `sp` those calls see: [`Vm::call`] puts its own
/// stack arguments below whatever `sp` says, so scratch placed at or under the
/// new `sp` would be overwritten by the first call that has more than four
/// arguments — and `CalcClipLCD` has twelve.
const SCRATCH: u32 = 0x40;

/// One pixel of the blend, from 0x5D958..0x5DA14.
///
/// The table is `Graphic::SetBlendingTable565`'s: a 32x32 byte matrix per
/// level, indexed `[src_component][dst_component]` with both components taken
/// to **five** bits — green included, which is why the green result is doubled
/// on the way out and the green weight runs to 64 while red and blue run to 32.
///
/// `ar > 31` is the fully-opaque case and takes the table value straight; the
/// mix below it is the shipped arithmetic, shifts and all. Every intermediate
/// is non-negative, so the guest's arithmetic shifts and these logical ones
/// agree.
#[inline(always)]
fn blend_table565(tbl: &[u8; 1024], src: u16, dst: u16, ar: u32, ag: u32, ab: u32) -> u16 {
    let (sr, sg, sb) = (
        (src >> 11) as usize,
        ((src & 0x07e0) >> 6) as usize,
        (src & 0x1f) as usize,
    );
    let (dr, dg, db) = (
        (dst >> 11) as u32,
        ((dst & 0x07e0) >> 6) as u32,
        (dst & 0x1f) as u32,
    );

    let tr = tbl[sr * 32 + dr as usize] as u32;
    let tg = tbl[sg * 32 + dg as usize] as u32 * 2;
    let tb = tbl[sb * 32 + db as usize] as u32;

    let (r, g, b) = if ar > 0x1f {
        (tr, tg, tb)
    } else {
        (
            (ar * tr + (0x20 - ar) * dr) >> 5,
            (ag * tg + (0x40 - ag) * (dg << 1)) >> 6,
            (ab * tb + (0x20 - ab) * db) >> 5,
        )
    };
    (((b & 0x1f) | (r << 11) | ((g << 5) & 0x07e0)) & 0xffff) as u16
}

/// `Graphic::DrawImageBlendingClip565` (0x5CAE8, 7,896 bytes) — the biggest
/// single lever left in this guest, and it answers three of the user's
/// complaints at once.
///
/// `(this, fb, x, y, w, h, img, srcX, srcY, xform, level, alpha, anchor)`, with
/// everything from `w` on the stack and `xform`/`level`/`alpha` narrowed to
/// bytes on the way in. Offsets below are `+0x…` from the entry.
///
/// # Six of its loops are ported, two are not
///
/// The function is a dozen specialised inner loops behind three nested
/// selectors: `level == 0x10` (+0x128), then the image's colour depth (+0x164),
/// then the transform — +0x178 for 8-bit sources, and for 4-bit ones the
/// +0x1B8 dispatcher block whose first transform test is +0x1EC.
///
/// The two 8-bit loops came first, on a `--profile-in` of the **new-game**
/// replay, which said the 4-bit branch was 2.6% and called most often. That
/// number was an artefact of the scene: new-game barely draws. Per-scene
/// differential profiling (ANALYSIS.md §31.4) put the *declined* 4-bit
/// transform-0x10 loop at:
///
/// ```text
///   scene            share of that scene's guest instructions
///   gameplay HUD     62.7%
///   intro/prologue   59%
///   title            42%
///   main menu        11%
/// ```
///
/// — so it was ported too, leaving `level == 0x10`, transforms 0x08 and 0x04 at
/// either depth, and 4-bit ROT180 interpreted. That was still 17.5% of the
/// ingame replay, from 13,836 declined calls, and the six bodies now here take
/// all of it but transform 0x04:
///
/// ```text
///   variant                  body     share of this function, ingame replay
///   4-bit  0x08 (MIRROR)    +0x1414   52.9%   <- ported here
///   8-bit  0x08 (MIRROR)    +0x1094   23.5%   <- ported here
///   4-bit  0x0c (ROT180)    +0x218     2.9%   <- ported here
///   8-bit  0x10 / 0x0c      +0xDC0 / +0xAD8   already ported
///   4-bit  0x10             +0x678            already ported
///   {4,8}-bit 0x04     +0x1978 / +0x125C      0, and still declined
/// ```
///
/// After the three this function leaves the profile entirely — the ingame
/// replay's top three, 33.7 / 21.8 / **17.5**, becomes 40.8 / 26.4 / — and all
/// five acceptance scenes get cheaper, at the shipped 585x270 canvas:
///
/// ```text
///   scene     guest instructions          declined
///   newgame    169.4 M -> 162.1 M  -4.3%     225 -> 0
///   ingame    1515.1 M -> 1252.2 M -17.4%  13,836 -> 166
///   shop       179.4 M -> 165.8 M  -7.6%     461 -> 0
///   online     160.4 M -> 152.8 M  -4.7%     255 -> 0
///   npcmenu    907.8 M -> 781.8 M -13.9%   7,658 -> 166
/// ```
///
/// Every remaining decline is transform 0x04, and only the two scenes that
/// reach it have any: `--profile-in` after the port finds nothing in this
/// function outside +0x1A00..+0x1D80, which is inside the 4-bit 0x04 body, and
/// it is 3.2 K samples against the 512 K of the new top entry. Of the ingame
/// 166, 164 provably enter +0x1978 and 2 are clipped away by `CalcClipLCD`
/// before the depth dispatcher runs, so their depth cannot be read off a
/// counter; none is 8-bit — the 8-bit dispatcher at +0x16C is entered 0 times
/// by a declined call.
///
/// **The body addresses are from the branch graph, not from proximity.** Every
/// intra-function branch was enumerated: each of the three bodies added here
/// has exactly one edge in, from its dispatcher, and its only edges out go to
/// the shared exit at +0x19C. That matters because ANALYSIS.md §32.6 records an
/// earlier session reading `--profile-in` here, seeing offsets that sat next to
/// an already-ported loop, and concluding the remaining cost was inside it —
/// when a declined path was branching back in. It also matters because the
/// compiler *scattered* the blocks: the 4-bit ROT180 body at +0x218 lends its
/// odd-`srcX` head to +0xF78, which sits between the two 8-bit bodies.
///
/// # Why it calls back into the guest twice
///
/// `CalcAnchor` (0x52FE4) and `CalcClipLCD` (0x52A48) run **once per blit**,
/// not per pixel. Transcribing 1,236 bytes of clipping to save a fraction of
/// the 7.5% that is not in the loops would spend the acceptance test's whole
/// margin for nothing, so they are called as they are. Both write only through
/// the out-pointers this function hands them, which is why scratch memory is
/// enough — and why declining *after* they have run is harmless: [`Vm::call`]
/// saves and restores the whole CPU, and the interpreted prologue then builds
/// its own frame over the scratch.
fn draw_image_blending_clip565(vm: &mut Vm) -> Result<bool, String> {
    let this = vm.arg(0);
    let fbp = vm.arg(1);
    let img = vm.arg(6);
    let xform = vm.arg(9) & 0xff;
    let level = vm.arg(10) & 0xff;
    let alpha = vm.arg(11) & 0xff;

    // Decline first, before touching anything: these three selectors pick the
    // inner loop, and every loop other than the six transcribed below has to
    // run interpreted. Deciding here also means the guest's own prologue —
    // which is where the anchor and clipping calls live — is not paid for a
    // call that is about to be handed back.
    // `level == 0x10` is **not a variant of this function, it is a prologue and
    // an epilogue around the ordinary one** — which is why declining it used
    // to cost so much. `+0x128 cmp fp,#0x10` / `+0x130 beq +0x124C` reaches
    // 0x5DD34, whose whole body is `Graphic::ChangeGrayPal(this, img)`
    // (0x582F4) followed by `b +0x134` — one instruction before the palette is
    // read — and the shared exit at `+0x19c` tests the same byte again and
    // calls `Graphic::RestorePal` (0x59400) at `+0x668`. So the image's palette
    // is greyed for the duration of the blit and put back afterwards, and the
    // draw in between is the loop this file already has.
    //
    // Measured: only 898 calls in the ingame replay take it, but they are large
    // ones — 70,044 interpreted row iterations and 2.67 M instructions in the
    // 4-bit inner loop, about **half** of everything left in this function. It
    // looked like a rare variant and was the biggest single decline in the file.
    let grey = level == 0x10;
    // `GbmImage::GetColorBit` (0x8529C) is `ldrb [img]`; the low nibble is the
    // depth. The function masks and tests it at +0x160..+0x168 and sends 4 to
    // the packed dispatcher at +0x1B8, everything else to +0x16C.
    let four_bit = vm.mem.read_u8(img + gbm::COLOR_BIT) as u32 & 0xf == 4;
    // Six of the eight (depth, transform) pairs are transcribed; the two left
    // out are transform 0x04, the 8-bit body at +0x125C and the 4-bit one at
    // +0x1978.
    //
    // The four transforms are a two-bit field with a spare state: 0x10 flips
    // neither axis, 0x08 flips x (+0x1094 / +0x1414), 0x04 flips y (+0x125C's
    // start index is `(y+h-1)*GetWidth() + x` with a +2 column step and a
    // `-GetWidth()` row step), and 0x0c flips both. So the missing one is not a
    // different kind of loop, it is the fourth corner of the same square — and
    // it is left out because it is not worth any risk: `--count` on the two
    // bodies over the ingame replay says the 4-bit one is entered 164 times and
    // the 8-bit one **never**, against 3.64 M calls to this function, and
    // neither shows up at all in `--profile-in`'s top twelve offsets.
    let ported = matches!(xform, xform::NORMAL | xform::MIRROR_X | xform::ROT180);
    if !ported {
        return Ok(false);
    }

    let mut x = vm.arg(2) as i32;
    let mut y = vm.arg(3) as i32;
    let mut w = vm.arg(4) as i32;
    let mut h = vm.arg(5) as i32;
    let anchor = vm.arg(12);

    // The shipped function's own guards, at +0x94 and +0xA8: a zero alpha or an
    // empty rectangle leaves without drawing.
    if alpha == 0 || w <= 0 || h <= 0 {
        return Ok(true);
    }

    // A greyed blit calls two guest helpers per blit — not per pixel — and the
    // first has to run **before** the palette is read, exactly where the guest
    // puts it (`+0x134` is one instruction before `GetPaletteID` at `+0x138`).
    let (grey_on, grey_off) = (
        vm.symbol("_ZN7Graphic13ChangeGrayPalEP8GbmImage"),
        vm.symbol("_ZN7Graphic10RestorePalEP8GbmImage"),
    );
    if grey && (grey_on.is_none() || grey_off.is_none()) {
        return Ok(false);
    }

    let (Some(calc_anchor), Some(calc_clip)) = (
        vm.symbol("_ZN7Graphic10CalcAnchorEPiS0_iii"),
        vm.symbol("_ZN7Graphic11CalcClipLCDEPiS0_S0_S0_S0_S0_iiiih"),
    ) else {
        return Ok(false);
    };
    let Some(skip_color) = vm.symbol("SKIP_COLOR") else {
        return Ok(false);
    };
    // `SKIP_COLOR` is a plain global int; the guest loads it once per blit
    // (+0x6A0 in the 4-bit body, +0xDE8 in the 8-bit NORMAL one, +0xAF8 in the
    // 8-bit ROT180 one — all the same GOT slot, +0x5C8) and compares it against
    // the half-word coming out of the palette.
    let skip = vm.mem.read_u32(skip_color);

    let fb = FbInfo::read(vm, fbp);
    let img_w = vm.mem.read_u16(img + gbm::WIDTH) as i32;
    let img_h = vm.mem.read_u16(img + gbm::HEIGHT) as i32;

    // ---- anchor and clip, in the guest -----------------------------------
    let sp0 = vm.cpu.reg_get(Mode::User, reg::SP);
    let s = (sp0 - SCRATCH) & !7;
    let (p_x, p_y, p_w, p_h, p_sx, p_sy) = (s, s + 4, s + 8, s + 12, s + 16, s + 20);
    vm.mem.write_u32(p_x, x as u32);
    vm.mem.write_u32(p_y, y as u32);
    vm.mem.write_u32(p_w, w as u32);
    vm.mem.write_u32(p_h, h as u32);
    let (arg_sx, arg_sy) = (vm.arg(7), vm.arg(8));
    vm.mem.write_u32(p_sx, arg_sx);
    vm.mem.write_u32(p_sy, arg_sy);

    vm.cpu.reg_set(Mode::User, reg::SP, s);
    let anchored = vm.call(calc_anchor, &[this, p_x, p_y, w as u32, h as u32, anchor]);
    let clipped = if anchored.is_ok() {
        vm.call(
            calc_clip,
            &[
                this,
                p_x,
                p_y,
                p_w,
                p_h,
                p_sx,
                p_sy,
                img_w as u32,
                img_h as u32,
                fb.width as u32,
                fb.height as u32,
                xform,
            ],
        )
    } else {
        Ok(0)
    };
    vm.cpu.reg_set(Mode::User, reg::SP, sp0);
    anchored?;
    // Non-zero means the blit is entirely off-screen.
    if clipped? != 0 {
        return Ok(true);
    }

    x = vm.mem.read_u32(p_x) as i32;
    y = vm.mem.read_u32(p_y) as i32;
    w = vm.mem.read_u32(p_w) as i32;
    h = vm.mem.read_u32(p_h) as i32;
    let sx = vm.mem.read_u32(p_sx) as i32;
    let sy = vm.mem.read_u32(p_sy) as i32;
    // Belt and braces, and known to be dead: `CalcClipLCD` returns 1 for any
    // non-positive `w` or `h` (+0x40..+0x54 load both through the out-pointers,
    // `ble` to the `mov r0,#1` at +0x98), and every transform branch funnels
    // through that test — so a zero return already means both are positive.
    if w <= 0 || h <= 0 {
        return Ok(true);
    }

    if grey {
        vm.call(grey_on.expect("checked above"), &[this, img])?;
    }

    // ---- the per-blit constants ------------------------------------------
    // The three weights all come from the single `alpha` argument at
    // +0x60..+0x84 and are then nudged by `Graphic::RevisionAlpha565`
    // (0x536F0), which adds 1, 2, 1 to the three bytes at `sp+0xac`. Red and
    // blue end up on a 0..32 scale and green on 0..64, matching the shifts in
    // `blend_table565`. The 4-bit body reads the same three bytes:
    // `ldrb r7,[sp,#0xac]` at +0x770, `[sp,#0xad]` at +0x7B8, `[sp,#0xae]` at
    // +0x7C8.
    let ar = (alpha >> 3) + 1;
    let ag = ((alpha & 0xf8) >> 2) + 2;
    let ab = ar;

    // The blending table lives inside `Graphic` itself, one 32x32 matrix per
    // level, the first at `this + 0x6D`. The 4-bit body forms the same address:
    // +0x6CC `lsl r3,sl,#5` (= level*32), then `add rX, r3, comp` and
    // `add rX, r4, rX, lsl #5`, then `ldrb [rX,#0x6d]`.
    //
    // **A greyed blit uses table 0, not table `level`.** The register that
    // indexes it is `sl`, and `+0x12c movne sl, fp` — the instruction that
    // copies `level` into it — is the one the grey branch skips: `+0x130 beq`
    // goes to `ChangeGrayPal` and returns to `+0x134`, past it. What `sl` still
    // holds there is `CalcClipLCD`'s return value from `+0x11c subs sl, r0, #0`,
    // and reaching this point at all means that was **zero**. Using `level`
    // here instead read 16 KB past the sixteen tables `SetBlendingTable565`
    // built, into the object's own fields, and drew the greyed minimap in
    // arbitrary colours — 180 frames of the ingame A/B, every frame from the
    // one the minimap first appears on.
    let tbl_level = if grey { 0 } else { level };
    let mut tbl = [0u8; 1024];
    let Some(src_tbl) = vm.mem.slice(this + 0x6d + tbl_level * 1024, 1024) else {
        return restore_and(vm, grey, grey_off, false);
    };
    tbl.copy_from_slice(src_tbl);

    // A 4-bit source is read **16 entries** deep, not 256 — not because the
    // allocation is known to be 32 bytes (nothing here reads
    // `GbmImage::GetPaletteSize`; the base comes from `GetPaletteID` at
    // +0x138), but because the packed loop can only ever form a nibble:
    // +0x730 `asr ip,sl,#4` and +0x734 `and sl,sl,#0xf`, and their twins at
    // +0x990 and +0xCAC. Asking for the 8-bit 512 bytes would read past a
    // 4-bit image's palette, and if that ever ended a mapped region `slice`
    // would return `None` and this port would decline for ever without saying
    // why — the failure most likely to look like "the port did nothing".
    let pal_entries = if four_bit { 16usize } else { 256usize };
    let mut pal = [0u16; 256];
    let pal_addr = vm.mem.read_u32(img + gbm::PALETTE);
    let Some(pal_bytes) = vm.mem.slice(pal_addr, pal_entries as u32 * 2) else {
        return restore_and(vm, grey, grey_off, false);
    };
    for (i, e) in pal.iter_mut().take(pal_entries).enumerate() {
        *e = u16::from_le_bytes([pal_bytes[i * 2], pal_bytes[i * 2 + 1]]);
    }
    let pixels = vm.mem.read_u32(img + gbm::PIXELS);

    // The destination stride is the frame buffer's *width in pixels*, not its
    // bytes-per-line: the shipped code indexes with `GetWidth()` and scales by
    // two at the load.
    let stride = fb.width;

    // ---- 4-bit source, transform 0x10 -------------------------------------
    // Dispatcher +0x1B8, body entry +0x678, per-blit constants +0x690..+0x6D8,
    // row loop head +0x6DC, pair loop +0x6F0.
    //
    // Two source pixels per byte, so the source row stride is `ceil(img_w/2)`
    // — +0x1C0 `tst ip,#1` with +0x1C8 `asreq r0,ip,#1` against +0x1D0/+0x1D4
    // `addne r3,r1,#1` / `asrne r3,r3,#1`. **Not `img_w/2`**: §31.4's
    // `asr fp,fp,#1` at +0x6F4 is the *output pair count*, a different
    // quantity, and using it as the stride skews each successive row by half a
    // pixel, cumulatively. The first byte of the blit is `sy*stride + (sx>>1)`
    // (+0x1E8 `asr r2,r3,#1`, +0x1F0 `mla r8,r1,r8,r2`), and the low bit of
    // `sx` that `asr` threw away is latched once at +0x6A8 (`and r3,r3,#1`),
    // re-tested at the top of every row (+0x6E4 `bne +0xC98`), and never
    // written inside the loop.
    //
    // The nibble order is settled twice over. Here: +0x718
    // `add r1,lr,ip,lsl #1` is `dst+0` while +0x710/+0x714 `add r0,ip,#1` /
    // `add r0,lr,r0,lsl #1` is `dst+2`; the **high** nibble (+0x730
    // `asr ip,sl,#4`) goes to `r1` at +0x850 and the **low** nibble (+0x734
    // `and sl,sl,#0xf`) to `r0` at +0x94C. Independently, the writer
    // `Graphic::GbmFillRect` (0x55110) uses the identical layout: `ceil(w/2)`
    // stride at +0x30..+0x40, `x>>1` byte index at +0x58, and at
    // +0x7C..+0xA4 `tst r3,#1` selects `andeq ip,ip,#0xf` + `orreq` (even
    // column: keep the low nibble, write the high) against
    // `bicne ip,ip,#0xf` + `orrne` (odd column), advancing the byte pointer
    // only after the odd one. **Even source x is the high nibble.** Swapping
    // the two transposes every pixel pair; ANALYSIS.md §31.7 is the record of
    // an investigation getting exactly this backwards.
    if four_bit && xform == xform::NORMAL {
        let row_bytes = if img_w & 1 != 0 { (img_w + 1) >> 1 } else { img_w >> 1 };
        let odd_sx = sx & 1 != 0;
        let mut src_row = sy.wrapping_mul(row_bytes).wrapping_add(sx >> 1);
        let mut dst_idx = y.wrapping_mul(stride).wrapping_add(x);

        for _ in 0..h {
            let mut src = src_row;
            let mut di = dst_idx;
            // +0x6DC..+0x6EC: the width the pair loop is given, reset every
            // row from `[sp,#0x7c]`.
            let mut wr = w;

            // +0xC98..+0xDBC. An odd `sx` lands mid-byte, so the row opens
            // with the **low** nibble of its first byte (+0xCA4 reads it,
            // +0xCAC masks `#0xf`). Then it re-enters the same pair loop one
            // byte on (+0xCA0 computes `row_start+1` and +0xCA8 stores it into
            // `[sp,#0x38]`, the per-row working pointer — `[sp,#0x48]`, the
            // per-row accumulator, is deliberately untouched, which is why the
            // +1 does not compound across rows), one destination pixel on
            // (+0xDB0) and one pixel short (+0xDB8 stores `w-1`). This is a
            // preamble to +0x6F0, not a second loop: +0xDBC is `b +0x6F0`.
            if odd_sx {
                let b = vm.mem.read_u8(pixels.wrapping_add(src as u32)) as usize;
                let src_px = pal[b & 0xf];
                if src_px as u32 != skip {
                    let at = fb.base.wrapping_add((di * 2) as u32);
                    let dst = vm.mem.read_u16(at);
                    vm.mem.write_u16(at, blend_table565(&tbl, src_px, dst, ar, ag, ab));
                }
                di += 1;
                src += 1;
                wr -= 1;
            }

            // +0x6F0..+0x95C: `wr/2` whole bytes, one source byte per output
            // pair, high nibble to `dst+0` and low nibble to `dst+2`. Each
            // nibble tests `SKIP_COLOR` for itself (+0x758 / +0x870) and both
            // skip branches land *before* the pointer bumps at +0x858 /
            // +0x954, so a skipped pixel still advances.
            let pairs = wr >> 1;
            for p in 0..pairs {
                let b = vm.mem.read_u8(pixels.wrapping_add((src + p) as u32)) as usize;
                let at = fb.base.wrapping_add(((di + p * 2) * 2) as u32);
                for (off, src_px) in [(0u32, pal[b >> 4]), (2u32, pal[b & 0xf])] {
                    if src_px as u32 == skip {
                        continue;
                    }
                    let at = at.wrapping_add(off);
                    let dst = vm.mem.read_u16(at);
                    vm.mem.write_u16(at, blend_table565(&tbl, src_px, dst, ar, ag, ab));
                }
            }
            di += pairs * 2;
            src += pairs;

            // +0x978..+0xA94. An odd remaining width leaves one pixel and it
            // **is** drawn, from the **high** nibble (+0x990 `asr r2,r2,#4`) of
            // the byte the pair loop stopped on. Dropping it loses one column
            // of every odd-width sprite.
            if wr & 1 != 0 {
                let b = vm.mem.read_u8(pixels.wrapping_add(src as u32)) as usize;
                let src_px = pal[b >> 4];
                if src_px as u32 != skip {
                    let at = fb.base.wrapping_add((di * 2) as u32);
                    let dst = vm.mem.read_u16(at);
                    vm.mem.write_u16(at, blend_table565(&tbl, src_px, dst, ar, ag, ab));
                }
            }

            // +0xA98..+0xAD4. The source row start walks by the packed stride
            // (+0xAC0) and the destination index by `GetWidth() - w` (+0xAC4)
            // on top of the `w` the row body already added. The within-row
            // advances — 1 for the odd-`sx` head, 2 per pair, 1 for the tail —
            // sum to exactly `w` in either parity, which is the only reason
            // this can step `dst_idx` by a plain `stride` and stay byte-exact.
            src_row = src_row.wrapping_add(row_bytes);
            dst_idx = dst_idx.wrapping_add(stride);
        }
        return restore_and(vm, grey, grey_off, true);
    }

    // ---- 4-bit source, mirrored: 0x08 at +0x1414 and 0x0c at +0x218 --------
    // Together these were the largest remaining decline in the file. On the
    // ingame replay at the shipped canvas, `--profile-in` put 52.9% of this
    // function inside the 0x08 body (+0x14C0 11.1%, +0x1600 10.0%, +0x1500
    // 9.9%, +0x15C0 8.6%, +0x16C0 6.6%, +0x1540 3.6%, +0x1640 3.1%) and 2.9%
    // inside the 0x0c one (+0x0300).
    //
    // **That attribution is by control flow, not by proximity** — the mistake
    // ANALYSIS.md §32.6 records. Every intra-function branch was enumerated:
    // +0x1414..+0x1978 has exactly one edge in, `+0x204 beq` from the 4-bit
    // dispatcher's `cmp r7,#8`, and two out, both to the shared exit at +0x19C;
    // +0x218..+0x668 has none in at all (the dispatcher *falls through*
    // `+0x214 bne`) and lends one block to +0xF78, which nothing else reaches.
    // So no already-ported loop can be branching back into either.
    //
    // The two bodies are the same code twice — see [`xform::MIRROR_X`] — and both
    // are the NORMAL 4-bit loop above with the destination reversed:
    //
    // * the row's first destination pixel is its **right**-hand end, `x+w-1`
    //   (+0x143C `add r0,r8,r0` then +0x1440 `sub r0,r0,#1`, against +0x23C and
    //   +0x244),
    // * the head, each pair and the tail all step the index **down**: +0x1968
    //   `sub sb,sb,#1` (head), +0x15EC `sub r1,r1,#4` and +0x16DC
    //   `sub r0,r0,#4` (pair), +0x1814 `sub sl,sl,#1` (tail),
    // * within a pair the **high** nibble is still the earlier source pixel and
    //   therefore lands to the **right** of the low one: +0x14C0
    //   `add r1,lr,ip,lsl #1` is the index itself and +0x14BC
    //   `add r0,lr,r0,lsl #1` is index−1, and +0x15E0 stores the high nibble's
    //   result through `r1`, +0x16D4 the low nibble's through `r0`.
    //
    // The index therefore falls by exactly `w` over a row in either parity,
    // which is what makes the single `dst_idx += row_delta` at the bottom
    // faithful: the guest adds `w ± GetWidth()` to a running counter, not
    // `± GetWidth()` to a row start.
    if four_bit {
        let row_bytes = if img_w & 1 != 0 { (img_w + 1) >> 1 } else { img_w >> 1 };
        let odd_sx = sx & 1 != 0;
        let mut src_row = sy.wrapping_mul(row_bytes).wrapping_add(sx >> 1);
        // +0x1418..+0x1448 (`mla sb,y,GetWidth(),x+w-1`) against +0x21C..+0x254,
        // where the row is `y+h-1` instead.
        let first_row = if xform == xform::MIRROR_X { y } else { y + h - 1 };
        let mut dst_idx = first_row
            .wrapping_mul(stride)
            .wrapping_add(x)
            .wrapping_add(w - 1);
        // +0x1458 `add fp,fp,r8` against +0x264 `rsb fp,fp,r8`.
        let row_delta = if xform == xform::MIRROR_X {
            w.wrapping_add(stride)
        } else {
            w.wrapping_sub(stride)
        };

        for _ in 0..h {
            let mut src = src_row;
            // +0x1484..+0x1498 / +0x290..+0x2A4: `[sp,#0x68]`, the width handed
            // to the pair loop, reset every row from `[sp,#0x7c]`.
            let mut wr = w;

            // +0x185C..+0x1974, and for 0x0c an out-of-line twin the compiler
            // parked at +0xF78 — in between the two 8-bit bodies, which is why
            // "the block after this one" is never a safe way to bound a body in
            // this function. Identical to the NORMAL path's head otherwise: an
            // odd `sx` opens the row on the **low** nibble of its first byte
            // and re-enters the pair loop one source byte on — `[sp,#0x30]`
            // gets `row_start+1` while `[sp,#0x48]`, the per-row accumulator,
            // is left alone, so the +1 does not compound across rows.
            if odd_sx {
                let b = vm.mem.read_u8(pixels.wrapping_add(src as u32)) as usize;
                let src_px = pal[b & 0xf];
                if src_px as u32 != skip {
                    let at = fb.base.wrapping_add((dst_idx * 2) as u32);
                    let dst = vm.mem.read_u16(at);
                    vm.mem.write_u16(at, blend_table565(&tbl, src_px, dst, ar, ag, ab));
                }
                dst_idx -= 1;
                src += 1;
                wr -= 1;
            }

            // +0x1498..+0x16E4 / +0x2A4..+0x4F0. As in the NORMAL loop each
            // nibble tests `SKIP_COLOR` for itself (+0x1504 / +0x1604) and both
            // skip branches land *before* the pointer step at +0x15EC /
            // +0x16DC, so a skipped pixel still advances.
            let pairs = wr >> 1;
            for p in 0..pairs {
                let b = vm.mem.read_u8(pixels.wrapping_add((src + p) as u32)) as usize;
                let at = fb.base.wrapping_add(((dst_idx - p * 2) * 2) as u32);
                for (back, src_px) in [(0u32, pal[b >> 4]), (2u32, pal[b & 0xf])] {
                    if src_px as u32 == skip {
                        continue;
                    }
                    let at = at.wrapping_sub(back);
                    let dst = vm.mem.read_u16(at);
                    vm.mem.write_u16(at, blend_table565(&tbl, src_px, dst, ar, ag, ab));
                }
            }
            dst_idx -= pairs * 2;
            src += pairs;

            // +0x1708..+0x1818 / +0x514..+0x624. An odd remaining width leaves
            // one pixel and it **is** drawn, from the **high** nibble (+0x1720
            // `asr r2,r2,#4`) of the byte the pair loop stopped on. The index
            // step at +0x1814 sits on this branch only, which is why the
            // per-row total is `w` and not `w+1`.
            if wr & 1 != 0 {
                let b = vm.mem.read_u8(pixels.wrapping_add(src as u32)) as usize;
                let src_px = pal[b >> 4];
                if src_px as u32 != skip {
                    let at = fb.base.wrapping_add((dst_idx * 2) as u32);
                    let dst = vm.mem.read_u16(at);
                    vm.mem.write_u16(at, blend_table565(&tbl, src_px, dst, ar, ag, ab));
                }
                dst_idx -= 1;
            }

            // +0x181C..+0x1858 / +0x628..+0x664: the packed source stride onto
            // `[sp,#0x48]` and `row_delta` onto the running index `[sp,#0x44]`.
            src_row = src_row.wrapping_add(row_bytes);
            dst_idx = dst_idx.wrapping_add(row_delta);
        }
        return restore_and(vm, grey, grey_off, true);
    }

    // ---- 8-bit source: +0xDC0 (0x10), +0x1094 (0x08) and +0xAD8 (0x0c) ----
    // One loop for three bodies, because the three differ only in which way the
    // two axes run. The x reversal is the destination pointer stepping by −2
    // (+0x1210 for 0x08, +0xC5C for 0x0c) where NORMAL steps by +2 (+0xF34);
    // the y reversal is the sign on `GetWidth()` in the row step — +0x1244
    // `add ip,ip,r3` for 0x08 and +0xF68 `rsb ip,fp,ip` for NORMAL both land on
    // the next row down, +0xC90 `rsb ip,r3,ip` for 0x0c on the previous one —
    // together with a start row of `y` against `y+h-1`.
    //
    // 0x08's body carried ~23.5% of this function on the ingame replay
    // (+0x1100 7.6%, +0x1140 5.8%, +0x11C0 5.1%, +0x1180 5.0%), and its range
    // 0x5DB7C..0x5DD34 likewise has exactly one edge in — `+0x188 beq` from the
    // 8-bit dispatcher — and two out, both to the shared exit.
    let rows_down = xform != xform::ROT180;
    let mirror_x = xform != xform::NORMAL;
    let mut row_src = pixels.wrapping_add((img_w * sy + sx) as u32);

    for r in 0..h {
        // Transform 0x0c starts at the bottom-right and walks back, so it
        // covers the same rectangle in the opposite order.
        let dy = if rows_down { y + r } else { y + h - 1 - r };
        let row_dst = fb.base.wrapping_add(((dy * stride + x) * 2) as u32);
        for c in 0..w {
            let src = pal[vm.mem.read_u8(row_src.wrapping_add(c as u32)) as usize];
            if src as u32 == skip {
                continue;
            }
            let p = row_dst.wrapping_add(((if mirror_x { w - 1 - c } else { c }) * 2) as u32);
            let dst = vm.mem.read_u16(p);
            vm.mem.write_u16(p, blend_table565(&tbl, src, dst, ar, ag, ab));
        }
        row_src = row_src.wrapping_add(img_w as u32);
    }
    restore_and(vm, grey, grey_off, true)
}

/// Put a greyed palette back, and return `handled`.
///
/// `Graphic::ChangeGrayPal` mutates the `GbmImage`'s palette in place, so every
/// path out of [`draw_image_blending_clip565`] after that call has to undo it —
/// including the two that decline. Missing one would grey a sprite permanently:
/// a corruption that outlives the frame and shows up in the A/B as a diff that
/// starts partway through a scene and never stops.
fn restore_and(
    vm: &mut Vm,
    grey: bool,
    restore: Option<u32>,
    handled: bool,
) -> Result<bool, String> {
    if grey {
        let f = restore.ok_or("blending: RestorePal vanished")?;
        let (this, img) = (vm.arg(0), vm.arg(6));
        vm.call(f, &[this, img])?;
    }
    Ok(handled)
}

/// `Graphic::ChangeFrameBufferBlending565` (0x53C88) — 12.9% of guest time in
/// gameplay, and the whole of it: this function has exactly one inner loop, so
/// unlike `DrawImageBlendingClip565` there is no variant to decline.
///
/// `(this, fb, colour, level, alpha)` with `alpha` on the stack, read as a
/// byte (`ldrb r5,[sp,#0x70]` at 0x53CA8). It tints the entire frame buffer
/// towards one colour through the `Graphic` object's 16 KB blend table — the
/// screen-wide flash a map effect uses.
///
/// **The patch goes on this symbol, not on the dispatcher.**
/// `Graphic::ChangeFrameBufferBlending` (0x54040) reads the pixel-format
/// selector at `this+0x5c` and *tail-branches* here (`b` at 0x54078), so `lr`
/// is still the original caller's and `sp` is still theirs — which is exactly
/// what [`Vm::run`]'s "return to lr" and `arg(4) = [sp]` need. Patching the
/// dispatcher instead would mean reimplementing that selector and hand-routing
/// the 666 case back to the interpreter, for seven instructions per full-screen
/// pass.
///
/// # The trap: this is *not* the same blend as `DrawImageBlendingClip565`
///
/// That one runs its alpha through `Graphic::RevisionAlpha565` (0x536F0) and
/// ends up with weights `(alpha>>3)+1` against `0x20-ar`. This one does not:
/// 0x53D30 is a bare `lsr r7,r7,#3` and the complement at 0x53D9C is
/// `0x1f - ar`. So the weights here sum to **31**, not 32, while the shift
/// still divides by 32 — every blended pixel is 31/32 of a normalised blend.
/// That reads as a bug and is not one; normalising it changes pixels, and the
/// acceptance test is that pixels do not change. Reusing [`blend_table565`]
/// here would be off by about one table step everywhere: invisible on a
/// screenshot, fatal to the 905-frame A/B.
fn change_frame_buffer_blending565(vm: &mut Vm) -> Result<bool, String> {
    let this = vm.arg(0);
    let fbp = vm.arg(1);
    let colour = vm.arg(2);
    // Read unmasked, exactly as the guest does (0x53CA4 `mov r8, r3`, no
    // `and #0xff`), even though the C++ parameter is `unsigned char`.
    let level = vm.arg(3);
    let alpha = vm.arg(4) & 0xff;

    // Nothing to do — and the shipped code writes nothing either, so this is a
    // completed call, not a decline (0x53CD0).
    if alpha == 0 {
        return Ok(true);
    }
    // The table region is 16 levels of 1 KB at `this+0x6d`
    // (`SetBlendingTable565`, 0x531A0). Every call site passes 0, so this can
    // only fire if an assumption is wrong — in which case the shipped code
    // should read its own garbage, not ours.
    if level > 15 {
        return Ok(false);
    }
    let Some(tbl) = vm.mem.slice(this + 0x6d + level * 1024, 1024) else {
        return Ok(false);
    };
    let tbl: [u8; 1024] = tbl.try_into().expect("1024 bytes");

    let fb = FbInfo::read(vm, fbp);
    let (w, h) = (fb.width, fb.height);
    if h <= 0 || w <= 0 {
        return Ok(true);
    }
    let Some(bytes) = (w as i64).checked_mul(h as i64).and_then(|n| n.checked_mul(2)) else {
        return Ok(false);
    };
    let Ok(bytes) = u32::try_from(bytes) else { return Ok(false) };

    // Alpha weights. Note there is no `RevisionAlpha565` on this path, and
    // that `ag`/`iag` are exactly twice `ar`/`iar` — which is why the green
    // term's `>>6` below is the same number as red's `>>5`.
    let ar = alpha >> 3; // 0..31        0x53D30
    let iar = 0x1f - ar; //              0x53D9C
    let opaque = ar > 0x1e; //           0x53DAC/0x53DE0 — i.e. alpha >= 0xF8

    // The constant colour, 0x00RRGGBB, packed to 565 and split into the three
    // five-bit table row indices (0x53CF8..0x53D3C).
    let c565 = ((colour >> 8) & 0xf800) | ((colour >> 5) & 0x07e0) | ((colour & 0xff) >> 3);
    let (s_r, s_g, s_b) = (c565 >> 11, (c565 & 0x07e0) >> 6, c565 & 0x1f);

    // Every output channel depends only on the matching input channel, and
    // everything else in the loop is invariant — so the whole per-pixel
    // multiply chain collapses into three 32-entry tables built once.
    //
    // The packing is deliberately `(v << 11) as u16` for red rather than
    // `(v & 0x1f) << 11`: the guest's merge at 0x53E24 is a bare
    // `orr ip, ip, r1, lsl #11` and only the `strh` truncates. Blue and green
    // *are* masked there (0x53E1C, 0x53E28). Here `v <= 277` for any table
    // byte, so `v << 11` never reaches bit 16 from below and the two forms
    // agree — but the code says what the guest says.
    let mut lut_r = [0u16; 32];
    let mut lut_g = [0u16; 32];
    let mut lut_b = [0u16; 32];
    for d in 0..32u32 {
        let t_r = tbl[(s_r * 32 + d) as usize] as u32;
        let t_g = tbl[(s_g * 32 + d) as usize] as u32 * 2; // 0x53DDC `lsl #1`
        let t_b = tbl[(s_b * 32 + d) as usize] as u32;
        let (rr, gg, bb) = if opaque {
            (t_r, t_g, t_b)
        } else {
            (
                (d * iar + ar * t_r) >> 5,
                // The guest keeps green at six bits throughout: `(d*2)` against
                // `iag = 2*iar`, `ag = 2*ar`, then `>>6`. That is exactly twice
                // the red expression, shifted one further, so it is written the
                // guest's way rather than folded.
                ((d * 2) * (iar * 2) + (ar * 2) * t_g) >> 6,
                (d * iar + ar * t_b) >> 5,
            )
        };
        lut_r[d as usize] = (rr << 11) as u16;
        lut_g[d as usize] = ((gg << 5) & 0x07e0) as u16;
        lut_b[d as usize] = (bb & 0x1f) as u16;
    }

    // One linear sweep. **The row pitch is `width`, not `bpl`**: the guest
    // rebuilds the row pointer as `base + pixoff*2` with `pixoff += width`
    // (0x53D98, 0x53E40) and never calls `GetBpl`. Using the stride would be
    // identical on this canvas and silently wrong on a padded one.
    let Some(buf) = vm.mem.slice_mut(fb.base, bytes) else { return Ok(false) };
    for px in buf.chunks_exact_mut(2) {
        let d = u16::from_le_bytes([px[0], px[1]]) as usize;
        let out = lut_r[d >> 11] | lut_g[(d & 0x07e0) >> 6] | lut_b[d & 0x1f];
        px.copy_from_slice(&out.to_le_bytes());
    }
    Ok(true)
}

/// `Graphic::FillRectBlending` (0x525AC) — 8.2% of gameplay once
/// `ChangeFrameBufferBlending565` is out of the way.
///
/// `(fb, x, y, w, h, level, alpha, rgb)`, the last four on the stack.
/// Geometrically it is [`fill_rect_alpha`]'s twin — same normalisation, same
/// clamps, same `base + y*bpl + x*bpp` origin, same `this[0x5c]` dispatch —
/// but the pixel maths is a different function: there is no arithmetic blend
/// at all, every output component comes out of the `Graphic` object's 32x32
/// blending table for `level`.
///
/// # Two traps, both of which make wrong code look right
///
/// **The argument order is `level` then `alpha`** (args 6 and 7), where
/// `FillRectAlpha` had only `alpha` at 6. Swapping them type-checks, draws
/// something plausible, and is wrong everywhere.
///
/// **[`blend_table565`] is not reusable**, though the packing and the table
/// indexing are identical. That helper came from `DrawImageBlendingClip565`,
/// which runs its alpha through `Graphic::RevisionAlpha565` (0x536F0) and so
/// weights the source by `(alpha>>3)+1`; this function does not (0x526DC is a
/// bare `lsr r6,r6,#3`). The *complements* coincide — `0x20-(a+1)` is
/// `0x1f-a` — which is exactly what makes them look interchangeable. They
/// differ by one whole table step in the source term, and by the opaque
/// threshold (`a > 0x1e` here, `ar > 0x1f` there). At `alpha = 0xff`, which
/// eight of the nine call sites pass, this function takes the opaque branch
/// and emits the table byte untouched while the other helper would mix.
fn fill_rect_blending(vm: &mut Vm) -> Result<bool, String> {
    let this = vm.arg(0);
    let fbp = vm.arg(1);
    let mut x = vm.arg(2) as i32;
    let mut y = vm.arg(3) as i32;
    let mut w = vm.arg(4) as i32;
    let mut h = vm.arg(5) as i32;
    let level = vm.arg(6) & 0xff;
    let alpha = vm.arg(7) & 0xff;
    let rgb = vm.arg(8);

    // Same decision, same reason, as `fill_rect_alpha`: only the 565 layout is
    // ported, because that is the one `nativeInitDisplay` selects and so the
    // only one the A/B can vouch for.
    if vm.mem.read_u16(this + 0x5c) as u32 & 0xff != 1 {
        return Ok(false);
    }
    // The table is 16 levels of 1 KB at `this+0x6d` (`SetBlendingTable565`,
    // 0x536A0). The guest applies no bound; observed call sites pass 0 or 3,
    // so this exists only so a wrong value runs the shipped code rather than
    // reading past the object here.
    if level > 15 {
        return Ok(false);
    }
    if alpha == 0 {
        return Ok(true);
    }

    // The three table rows the constant colour selects. Only 96 bytes are
    // needed — unlike `DrawImageBlendingClip565`, the source components are
    // loop-invariant here.
    let t = this + 0x6d + level * 1024;
    let (sr, sg, sb) = (
        (rgb & 0x00ff_0000) >> 19,
        (rgb & 0x0000_ff00) >> 11,
        (rgb & 0x0000_00ff) >> 3,
    );
    let mut row = [[0u8; 32]; 3];
    for (n, s) in [sr, sg, sb].iter().enumerate() {
        let Some(r) = vm.mem.slice(t + s * 32, 32) else { return Ok(false) };
        row[n].copy_from_slice(r);
    }

    let fb = FbInfo::read(vm, fbp);
    if x < 0 {
        w += x;
        x = 0;
    }
    if y < 0 {
        h += y;
        y = 0;
    }
    if x + w > fb.width {
        w = fb.width - x;
    }
    if y + h > fb.height {
        h = fb.height - y;
    }
    if h <= 0 || w <= 0 {
        return Ok(true);
    }

    let a = alpha >> 3; // 0..31, no +1     0x526DC
    let ag = (alpha & 0xf8) >> 2; // 0..62, no +2   0x526CC
    let opaque = a > 0x1e; //                0x52748/0x5277C

    // Each output channel depends only on the matching input channel and on
    // loop invariants, so the whole per-pixel chain collapses into three
    // 32-entry tables holding the already-shifted contribution. Red is left
    // unmasked because the guest leaves it unmasked (0x527C4 is a bare
    // `orr ... lsl #11`); `SetBlendingTable565` saturates entries to 0..31 so
    // it cannot matter, but the code says what the guest says.
    let mut lut = [[0u16; 32]; 3];
    for k in 0..32usize {
        let (tr, tg, tb) = (row[0][k] as u32, row[1][k] as u32 * 2, row[2][k] as u32);
        let d = k as u32;
        let (rr, gg, bb) = if opaque {
            (tr, tg, tb)
        } else {
            (
                (a * tr + (0x1f - a) * d) >> 5,
                (ag * tg + (0x3e - ag) * (d * 2)) >> 6,
                (a * tb + (0x1f - a) * d) >> 5,
            )
        };
        lut[0][k] = (rr << 11) as u16;
        lut[1][k] = ((gg << 5) & 0x07e0) as u16;
        lut[2][k] = (bb & 0x1f) as u16;
    }

    // Rows advance by `bpl`, not by `width*2` — this one really does use the
    // stride (0x52800 `add r5,r5,r0` with r0 from `GetBpl`), unlike
    // `ChangeFrameBufferBlending565`, which walks the buffer contiguously.
    let mut rowp = fb.base.wrapping_add((y * fb.bpl + x * fb.bpp_bytes) as u32);
    for _ in 0..h {
        for i in 0..w {
            let p = rowp.wrapping_add((i * 2) as u32);
            let d = vm.mem.read_u16(p) as usize;
            let out = lut[0][d >> 11] | lut[1][(d & 0x07e0) >> 6] | lut[2][d & 0x1f];
            vm.mem.write_u16(p, out);
        }
        rowp = rowp.wrapping_add(fb.bpl as u32);
    }
    Ok(true)
}

/// `Graphic::DrawImageStretchClip565` (0x5A188, 1,500 bytes) and its twin
/// `Graphic::DrawImageStretchClip565R` (0x584A0, 1,464 bytes) — together
/// **76% of a cutscene frame**, and a cutscene frame is the most expensive
/// thing this game draws: 1.50 M guest instructions, 2.5x gameplay
/// (`ANALYSIS.md` §31.4). One host body serves both; see "the twin" below.
///
/// `(this, fb, x, y, srcW, srcH, img, srcX, srcY, xform, xPct, yPct, level,
/// alpha, anchor)` — everything from `srcW` on the stack, and `xform`,
/// `level`, `alpha` read as bytes. **Arguments 10 and 11 are percentages**,
/// not a destination rectangle: the destination is `srcW*xPct/100` by
/// `srcH*yPct/100` (0x5A22C, 0x5A270, both the GCC divide-by-100 magic
/// `0x51EB851F`). Mistaking them for a width and a height type-checks and
/// draws a plausible sprite at the wrong size.
///
/// # What is *not* here
///
/// * **No `CalcClipLCD`.** Unlike [`draw_image_blending_clip565`] this
///   function clips per pixel against a rectangle that lives in the `Graphic`
///   object — `this+0x40E0/E4/E8/EC`, half-open `[x0,x1) x [y0,y1)` — written
///   only by `Graphic::SetClipGbm` (0x520E0) and `Graphic::ResetClipGbm`
///   (0x521C4, which fills it with the frame buffer's own extent). It is a
///   different rectangle from the `MC_GrpContext` clip at `this+0xC` that
///   `Graphic::SetClip` (0x5FB54) writes, and from anything `CalcClipLCD`
///   returns. Reading either of those instead would clip against the wrong
///   box, and on this canvas all three are usually full-screen, so it would
///   look right.
/// * **No source bounds test at all.** The `+1` on each step (below) lets the
///   last column sample one pixel past the row; the shipped code reads it, so
///   this reads it too.
///
/// # The stepping, transcribed with its `+1`
///
/// Both axes are 10.10 fixed point (0x5A2A0, 0x5A2B0: `__divsi3` of
/// `src << 10` by the destination extent), and both accumulators start at 0.
/// **The increment the loops use is the quotient plus one** — 0x5A3A0
/// `add r8, r3, #1` for X and 0x5A370 `add r2, r2, #1` for Y — so a blit walks
/// the source slightly faster than `srcW/dw` and the last column can land on
/// `srcW` itself. That is not a rounding choice this port gets to make; drop
/// the `+1` and every stretched sprite in the game shifts.
///
/// # The blend is `blend_table565`, and here that is the right helper
///
/// This is the opposite of the trap documented on
/// [`change_frame_buffer_blending565`]. That one weights the source by a bare
/// `alpha>>3`; this one builds `{alpha>>3, (alpha&0xF8)>>2, alpha>>3}` on the
/// stack and hands it to `Graphic::RevisionAlpha565` (0x536F0), which adds
/// 1, 2, 1 — giving the same `(alpha>>3)+1` / `((alpha&0xF8)>>2)+2` weights,
/// the same `0x20-ar` complement, and the same `ar > 0x1F` opaque threshold
/// (0x5A54C) that [`blend_table565`] already implements. The table indexing is
/// identical too: `this + 0x6D + level*1024 + component*32`.
///
/// # The twin
///
/// `...565R` differs in exactly three places, all of them one transpose:
///
/// * the destination walks **down** a column per source column and **across**
///   by one pixel per source row (0x58744/0x58960 against 0x5A398/0x5A63C),
/// * `CalcAnchor` is handed `(dh, dw)` instead of `(dw, dh)` (0x58640 vs
///   0x5A31C), so the centring transposes with the image,
/// * the Y flip is inverted — `xform` 4 and 0xC do *not* flip in the R
///   variant and everything else does (0x586EC..0x586FC, 0x58A00), the exact
///   complement of the non-R rule (0x5A3D8, 0x5A3DC).
///
/// Everything else — the percentages, the `+1` steps, the palette fetch, the
/// `SKIP_COLOR` test, the four clip fields, the blend — is
/// instruction-for-instruction the same, which is why this is one body and a
/// `bool` rather than two.
///
/// # What it declines
///
/// Only `level >= 0x10`. `level == 0x10` is a different function wearing this
/// one's name: 0x5A294 branches to 0x5A744, which calls
/// `Graphic::ChangeGrayPal` (0x582F4) on the way in and
/// `Graphic::RestorePal` (0x59400) on the way out (0x5A6DC), mutating the
/// image's palette around the blit — and the R twin calls `ChangeGrayPal`
/// (0x58A40) without ever restoring, which is one more reason to leave that
/// path to the interpreter. `level > 0x10` would index past the 16 KB of
/// blending tables at `this+0x6D` into the object's other fields. In the
/// cutscene replay `ChangeGrayPal` is entered **0 times**, so neither is a
/// path worth transcribing.
///
/// Nothing else declines, and that is a measurement, not an omission. All four
/// `xform` values occur — non-R sees 0x10/0x8/0xC/0x4 at 1602/879/201/132
/// calls and R at 276/158/290/402 — but every one of them reduces to two
/// booleans, so there is no variant left to hand back. Both colour depths are
/// served for the same reason: 3,431 of the 3,940 cutscene calls take the
/// 4-bit fetch at 0x5A668..0x5A694, and a build that declined it saved 0.6%
/// where this one saves 50.5%. Copying
/// [`draw_image_blending_clip565`]'s 4-bit decline here would have bought
/// almost nothing.
fn draw_image_stretch_clip565(vm: &mut Vm) -> Result<bool, String> {
    stretch_clip565(vm, false)
}

/// The transposed twin. See [`draw_image_stretch_clip565`].
fn draw_image_stretch_clip565r(vm: &mut Vm) -> Result<bool, String> {
    stretch_clip565(vm, true)
}

fn stretch_clip565(vm: &mut Vm, reversed: bool) -> Result<bool, String> {
    let this = vm.arg(0);
    let fbp = vm.arg(1);
    let img = vm.arg(6);
    let xform = vm.arg(9) & 0xff;
    let level = vm.arg(12) & 0xff;
    let alpha = vm.arg(13) & 0xff;

    // Decline first, before reading anything else: `level == 0x10` is the
    // ChangeGrayPal/RestorePal path (0x5A294 -> 0x5A744, 0x5A6DC) and anything
    // above it indexes past the blending tables. The interpreter runs both
    // exactly as shipped.
    if level >= 0x10 {
        return Ok(false);
    }

    let mut x = vm.arg(2) as i32;
    let mut y = vm.arg(3) as i32;
    let src_w = vm.arg(4) as i32;
    let src_h = vm.arg(5) as i32;
    let src_x = vm.arg(7) as i32;
    let src_y = vm.arg(8) as i32;
    let x_pct = vm.arg(10) as i32;
    let y_pct = vm.arg(11) as i32;
    let anchor = vm.arg(14);

    // The destination extent, as percentages of the source rectangle. The
    // shipped code leaves on a zero alpha or a zero destination width
    // (0x5A24C..0x5A268) and then on a zero destination height (0x5A284) —
    // before the divides, which is what keeps them from dividing by zero.
    let dw = src_w.wrapping_mul(x_pct) / 100;
    if alpha == 0 || dw == 0 {
        return Ok(true);
    }
    let dh = src_h.wrapping_mul(y_pct) / 100;
    if dh == 0 {
        return Ok(true);
    }

    // 10.10 fixed point, and the `+1` the loops actually add — see the note
    // above. `wrapping_div` only for `i32::MIN / -1`; `__divsi3` truncates
    // toward zero exactly as Rust's `/` does.
    let x_step = src_w.wrapping_shl(10).wrapping_div(dw).wrapping_add(1);
    let y_step = src_h.wrapping_shl(10).wrapping_div(dh).wrapping_add(1);

    // `Graphic::RevisionAlpha565` (0x536F0) inlined: it adds 1, 2, 1 to the
    // three bytes built at 0x5A1F4..0x5A218 and returns. Same weights as
    // `draw_image_blending_clip565`, hence the same `blend_table565`.
    let ar = (alpha >> 3) + 1;
    let ag = ((alpha & 0xf8) >> 2) + 2;
    let ab = ar;

    let colour_bit = vm.mem.read_u8(img + gbm::COLOR_BIT) as u32 & 0xf;
    let four_bit = colour_bit == 4;

    // `GbmImage::GetWidth` is the row pitch, and for a 4-bit image the shipped
    // code replaces it with the pitch in *bytes* — rounding up, not down
    // (0x5A710..0x5A728) — and halves `srcX` with it (0x5A734).
    let img_w = vm.mem.read_u16(img + gbm::WIDTH) as i32;
    let (row_pitch, src_base) = if four_bit {
        let pitch = if img_w & 1 != 0 { (img_w + 1) >> 1 } else { img_w >> 1 };
        (pitch, src_y.wrapping_mul(pitch).wrapping_add(src_x >> 1))
    } else {
        (img_w, src_y.wrapping_mul(img_w).wrapping_add(src_x))
    };

    let Some(calc_anchor) = vm.symbol("_ZN7Graphic10CalcAnchorEPiS0_iii") else {
        return Ok(false);
    };
    let Some(skip_color) = vm.symbol("SKIP_COLOR") else {
        return Ok(false);
    };
    let skip = vm.mem.read_u32(skip_color);

    let fb = FbInfo::read(vm, fbp);

    // ---- anchor, in the guest ---------------------------------------------
    // 26 instructions, once per blit, and it writes nothing but the two ints
    // it is handed — so scratch below the caller's frame is enough, exactly as
    // in `draw_image_blending_clip565`. **The R variant passes `(dh, dw)`**
    // (0x58640/0x58644); the non-R one passes `(dw, dh)` (0x5A31C/0x5A320).
    let sp0 = vm.cpu.reg_get(Mode::User, reg::SP);
    let s = (sp0 - SCRATCH) & !7;
    let (p_x, p_y) = (s, s + 4);
    vm.mem.write_u32(p_x, x as u32);
    vm.mem.write_u32(p_y, y as u32);
    let (a3, a4) = if reversed { (dh, dw) } else { (dw, dh) };
    vm.cpu.reg_set(Mode::User, reg::SP, s);
    let anchored = vm.call(calc_anchor, &[this, p_x, p_y, a3 as u32, a4 as u32, anchor]);
    vm.cpu.reg_set(Mode::User, reg::SP, sp0);
    anchored?;
    x = vm.mem.read_u32(p_x) as i32;
    y = vm.mem.read_u32(p_y) as i32;

    // ---- per-blit constants ----------------------------------------------
    let mut tbl = [0u8; 1024];
    let Some(src_tbl) = vm.mem.slice(this + 0x6d + level * 1024, 1024) else {
        return Ok(false);
    };
    tbl.copy_from_slice(src_tbl);

    // 4-bit indices reach 15, byte indices 255 — read only what the shipped
    // code can index, so a short palette allocation does not turn into a
    // decline for the depth that never touches the rest of it.
    let pal_addr = vm.mem.read_u32(img + gbm::PALETTE);
    let pal_entries: u32 = if four_bit { 16 } else { 256 };
    let mut pal = [0u16; 256];
    let Some(pal_bytes) = vm.mem.slice(pal_addr, pal_entries * 2) else {
        return Ok(false);
    };
    for (i, e) in pal.iter_mut().take(pal_entries as usize).enumerate() {
        *e = u16::from_le_bytes([pal_bytes[i * 2], pal_bytes[i * 2 + 1]]);
    }
    let pixels = vm.mem.read_u32(img + gbm::PIXELS);

    // The clip rectangle that lives in the object, not in the context.
    let clip_x0 = vm.mem.read_u32(this + 0x40e0) as i32;
    let clip_y0 = vm.mem.read_u32(this + 0x40e4) as i32;
    let clip_x1 = vm.mem.read_u32(this + 0x40e8) as i32;
    let clip_y1 = vm.mem.read_u32(this + 0x40ec) as i32;

    // The transform is two booleans, total over every `xform` value the guest
    // can be handed — which is why nothing declines here. X: 0x5A648/0x5A430
    // and 0x58968/0x5874C, identical in both variants. Y: 0x5A3D8/0x5A3DC in
    // the non-R body, and its exact complement at 0x586EC..0x586FC and
    // 0x58A00 in the R one.
    let flip_x = xform == 8 || xform == 0xc;
    let flip_y = reversed != (xform == 4 || xform == 0xc);

    // The destination pitch is `FrameBufferInfo::GetWidth()` — the width in
    // pixels — not `GetBpl()` (0x5A398, 0x58724). Same as
    // `draw_image_blending_clip565`, and silently wrong on a padded surface.
    let stride = fb.width;
    let origin = fb
        .base
        .wrapping_add((y.wrapping_mul(stride).wrapping_add(x).wrapping_shl(1)) as u32);

    // One loop nest for both variants. The outer index steps the source rows;
    // in the R variant that walks the destination *across*, so it is the
    // destination **x** the outer index moves and the clip pair swaps with it.
    let (outer_base, inner_base) = if reversed { (x, y) } else { (y, x) };
    let (outer_lo, outer_hi) = if reversed { (clip_x0, clip_x1) } else { (clip_y0, clip_y1) };
    let (inner_lo, inner_hi) = if reversed { (clip_y0, clip_y1) } else { (clip_x0, clip_x1) };
    let outer_step = if reversed { 2 } else { stride.wrapping_mul(2) };
    let inner_step = if reversed { stride.wrapping_mul(2) } else { 2 };

    // The four clip comparisons are two half-open range tests, and each range
    // is invariant along one loop — so they hoist out of the per-pixel body
    // the way `copy_frame_buffer`'s do. The iterations that drop out wrote
    // nothing, and each accumulator is `index * step`, so nothing carries.
    let o0 = 0.max(outer_lo.saturating_sub(outer_base));
    let o1 = dh.min(outer_hi.saturating_sub(outer_base));
    let i0 = 0.max(inner_lo.saturating_sub(inner_base));
    let i1 = dw.min(inner_hi.saturating_sub(inner_base));
    if o0 >= o1 || i0 >= i1 {
        return Ok(true);
    }

    for o in o0..o1 {
        let mut sy = o.wrapping_mul(y_step) >> 10;
        if flip_y {
            sy = (src_h - 1).wrapping_sub(sy);
        }
        let src_row = pixels.wrapping_add(src_base.wrapping_add(row_pitch.wrapping_mul(sy)) as u32);
        let dst_row = origin.wrapping_add(o.wrapping_mul(outer_step) as u32);
        for i in i0..i1 {
            let mut sx = i.wrapping_mul(x_step) >> 10;
            if flip_x {
                sx = (src_w - 1).wrapping_sub(sx);
            }
            // 8-bit: one byte per pixel. 4-bit: two, **high nibble first** —
            // `tst r0,#1` / `asreq r0,r0,#4` at 0x5A66C..0x5A680, so an even
            // source column takes the top half of the byte.
            let src = if four_bit {
                let b = vm.mem.read_u8(src_row.wrapping_add((sx >> 1) as u32));
                pal[(if sx & 1 == 0 { b >> 4 } else { b & 0xf }) as usize]
            } else {
                pal[vm.mem.read_u8(src_row.wrapping_add(sx as u32)) as usize]
            };
            if src as u32 == skip {
                continue;
            }
            let p = dst_row.wrapping_add(i.wrapping_mul(inner_step) as u32);
            let dst = vm.mem.read_u16(p);
            vm.mem.write_u16(p, blend_table565(&tbl, src, dst, ar, ag, ab));
        }
    }
    Ok(true)
}

/// `Graphic::DrawImageMirrorClip565` (0x56E18, 2,500 bytes) — **21.8% of guest
/// time in the ingame replay**, the largest entry left after the font stack,
/// and none of it was ported before this.
///
/// `(this, fb, x, y, w, h, img, srcX, srcY, xform, anchor)` — everything from
/// `w` on the stack, and `xform` narrowed to a byte on the way in
/// (`+0x20 ldrb r8,[sp,#0xa4]`). Offsets below are `+0x…` from the entry.
///
/// # It is a colour-keyed copy, not a blend
///
/// There is no `level` and no `alpha` in the signature and no blending table
/// anywhere in the body: a source pixel either lands verbatim or, if the
/// palette produced `SKIP_COLOR`, is dropped (`+0x1c4 cmp lr,r4` / `strhne`,
/// and its seven twins). So [`blend_table565`] has no business here — reaching
/// for it because the neighbouring ports use it would tint every mirrored
/// sprite in the game.
///
/// # Eight arms, one body
///
/// The function is the image's colour depth (`+0x114 cmp r0,#4`) times the four
/// transform values (`+0x128`/`+0x134`/`+0x13c`/`+0x144` for 8-bit,
/// `+0x230`/`+0x240`/`+0x248`/`+0x250` for 4-bit). Transcribing the eight
/// separately would be eight chances to get one wrong, so they were read
/// against each other first, and they differ in exactly two signs:
///
/// ```text
///   xform  first destination pixel     per px  per row  arm (8-bit / 4-bit)
///   0x10   y*stride + x                  +1    +stride  +0x460 / +0x590
///   0x08   y*stride + x+w-1              -1    +stride  +0x3c0 / +0x6f0
///   0x04   (y+h-1)*stride + x            +1    -stride  +0x4f0 / +0x858
///   0x0c   (y+h-1)*stride + x+w-1        -1    -stride  +0x14c / +0x258
/// ```
///
/// The per-row step is not one addition in the shipped code — each arm
/// accumulates `-w`/`+w` across the row body and then adds `stride+w`,
/// `w-stride` or `-(w+stride)` at the row tail to land on `±stride`. Both
/// parities of the 4-bit row body add up to exactly `w` (see the pair-count
/// note below), which is the only reason this can be written as
/// `dy*stride + dx` and stay byte-exact. The source walk is the same in all
/// eight: forward by one pixel per column, and by one row pitch per row
/// (`add r1,r1,r5` for 8-bit, `add r5,r5,[sp,#0x2c]` for 4-bit).
///
/// So `flip_x` and `flip_y` are the whole transform, and nothing declines for
/// it. The one value that does decline is a `xform` outside those four: the
/// shipped code falls out at `+0x1fc` without drawing, and the interpreter can
/// keep that.
///
/// # All eight arms are covered, and `--profile-in` says otherwise
///
/// The acceptance replays enter every one of them. That is worth writing down
/// because the obvious way to check it is wrong: `--profile-in` prints a
/// **top-twelve** offset list, and the three rare arms sit under the cutoff, so
/// they read as "never entered" and this port nearly declined them for want of
/// a test. `--count` on the arm entry addresses cannot hide anything, and over
/// the ingame scene it says:
///
/// ```text
///   arm            entry     ingame   npcmenu    | shows in --profile-in?
///   8-bit NORMAL   0x57278   30,100    23,505    | yes
///   4-bit NORMAL   0x573A8   29,300    16,639    | yes
///   4-bit MIRROR_X 0x57508   14,285     7,150    | yes
///   8-bit MIRROR_X 0x571D8    8,863     8,545    | yes
///   4-bit ROT180   0x57070    1,825       831    | yes
///   4-bit MIRROR_Y 0x57670    1,387       670    | **no**
///   8-bit ROT180   0x56F64      283       283    | **no**
///   8-bit MIRROR_Y 0x57308      219       219    | **no**
/// ```
///
/// (`newgame`/`shop`/`online` only ever reach the two NORMAL arms, plus a
/// handful of 4-bit MIRROR_X and ROT180 in `newgame`.)
///
/// The three rare arms were additionally checked one call at a time, because
/// 219 calls inside a 1,241-frame diff is thin evidence. A scaffold in this
/// function forced `xform` to each of the four values in turn and, for every
/// 29th call of the ingame replay, ran the shipped code — patch lifted out of
/// [`Vm::patches`] for the duration of that one call — and then the port, on
/// byte-identical inputs, and compared the **whole frame buffer**:
/// 4 x 4,500 sampled calls (1,467 8-bit, 3,033 4-bit each) at the shipped
/// 585x270 canvas, **zero mismatches**. Redo it by reinstating that scaffold;
/// it is not kept here because it doubles the cost of every blit.
///
/// # The 4-bit nibble order, and the row pitch
///
/// Two source pixels per byte, so the pitch is `ceil(img_w/2)`
/// (`+0x20c tst r5,#1` / `addne` / `asr`) — **not** `img_w/2` — and the first
/// byte of the blit is `pitch*srcY + (srcX>>1)` (`+0x22c asr r2,r3,#1`,
/// `+0x238 mla r5,r1,r5,r2`). The low bit of `srcX` that the `asr` threw away
/// is latched once per arm (`and r3,r3,#1`) and re-tested at the top of every
/// row, where an odd `srcX` opens with the **low** nibble of the first byte and
/// then re-enters the pair loop one byte and one destination pixel on, one
/// pixel short (`+0x2d0`, `+0x604`, `+0x764`, `+0x8d0`).
///
/// **Even source x is the high nibble** — the same fact
/// [`draw_image_blending_clip565`] documents at length, and the pair loops here
/// agree: `asr lr,r2,#4` goes to the destination pixel the loop is on and
/// `and r2,r2,#0xf` to the next one along `dstep`. An odd remaining width
/// leaves one pixel and it **is** drawn, from the **high** nibble.
///
/// One byte, two writes: the shipped pair loop loads the source byte once and
/// stores the high nibble before the low one. This transcription keeps that
/// order rather than treating a pair as two independent pixels — the two only
/// differ if a source ever aliases the destination, and the point of this file
/// is not to decide that it cannot.
///
/// # Why it calls back into the guest twice
///
/// `CalcAnchor` (0x52FE4) at `+0x84` and `CalcClipLCD` (0x52A48) at `+0xcc` run
/// once per blit, and both write only through the out-pointers they are handed
/// — the same reason [`draw_image_blending_clip565`] calls them instead of
/// transcribing them, with the same [`SCRATCH`] below the caller's frame. A
/// non-zero `CalcClipLCD` means the blit is entirely off-screen
/// (`+0xdc bne +0x1fc`).
fn draw_image_mirror_clip565(vm: &mut Vm) -> Result<bool, String> {
    let this = vm.arg(0);
    let fbp = vm.arg(1);
    let img = vm.arg(6);
    let xform = vm.arg(9) & 0xff;

    // Decline first, before touching anything: an unrecognised transform falls
    // out of the shipped function at +0x1fc having drawn nothing, and the
    // interpreter reproduces that for free. The four that are handled reduce to
    // two signs — see the table above.
    let (flip_x, flip_y) = match xform {
        xform::NORMAL => (false, false),
        xform::MIRROR_X => (true, false),
        xform::MIRROR_Y => (false, true),
        xform::ROT180 => (true, true),
        _ => return Ok(false),
    };

    let (Some(calc_anchor), Some(calc_clip)) = (
        vm.symbol("_ZN7Graphic10CalcAnchorEPiS0_iii"),
        vm.symbol("_ZN7Graphic11CalcClipLCDEPiS0_S0_S0_S0_S0_iiiih"),
    ) else {
        return Ok(false);
    };
    // `SKIP_COLOR` is a plain global int, loaded once per blit through the same
    // GOT slot (+0x5C8) every arm uses, and compared against the half-word
    // coming out of the palette.
    let Some(skip_color) = vm.symbol("SKIP_COLOR") else {
        return Ok(false);
    };
    let skip = vm.mem.read_u32(skip_color);

    let mut x = vm.arg(2) as i32;
    let mut y = vm.arg(3) as i32;
    let mut w = vm.arg(4) as i32;
    let mut h = vm.arg(5) as i32;
    let anchor = vm.arg(10);

    // `FrameBufferInfo`'s constructor at +0x24 has no side effects, so the
    // descriptor is read directly; `GetWidth`/`GetHeight`/`GetFrameBufPtr` are
    // `ldr [this,#8]`/`[#0xc]`/`[#0x10]` on that snapshot, which is the
    // descriptor's width, height and base.
    let fb = FbInfo::read(vm, fbp);
    let img_w = vm.mem.read_u16(img + gbm::WIDTH) as i32;
    let img_h = vm.mem.read_u16(img + gbm::HEIGHT) as i32;

    // ---- anchor and clip, in the guest -----------------------------------
    let sp0 = vm.cpu.reg_get(Mode::User, reg::SP);
    let s = (sp0 - SCRATCH) & !7;
    let (p_x, p_y, p_w, p_h, p_sx, p_sy) = (s, s + 4, s + 8, s + 12, s + 16, s + 20);
    vm.mem.write_u32(p_x, x as u32);
    vm.mem.write_u32(p_y, y as u32);
    vm.mem.write_u32(p_w, w as u32);
    vm.mem.write_u32(p_h, h as u32);
    let (arg_sx, arg_sy) = (vm.arg(7), vm.arg(8));
    vm.mem.write_u32(p_sx, arg_sx);
    vm.mem.write_u32(p_sy, arg_sy);

    vm.cpu.reg_set(Mode::User, reg::SP, s);
    let anchored = vm.call(calc_anchor, &[this, p_x, p_y, w as u32, h as u32, anchor]);
    let clipped = if anchored.is_ok() {
        vm.call(
            calc_clip,
            &[
                this,
                p_x,
                p_y,
                p_w,
                p_h,
                p_sx,
                p_sy,
                img_w as u32,
                img_h as u32,
                fb.width as u32,
                fb.height as u32,
                xform,
            ],
        )
    } else {
        Ok(0)
    };
    vm.cpu.reg_set(Mode::User, reg::SP, sp0);
    anchored?;
    if clipped? != 0 {
        return Ok(true);
    }

    x = vm.mem.read_u32(p_x) as i32;
    y = vm.mem.read_u32(p_y) as i32;
    w = vm.mem.read_u32(p_w) as i32;
    h = vm.mem.read_u32(p_h) as i32;
    let sx = vm.mem.read_u32(p_sx) as i32;
    let sy = vm.mem.read_u32(p_sy) as i32;

    // Every arm opens `cmp <h>,#0 / ble +0x1fc`, so a non-positive height
    // leaves without drawing.
    if h <= 0 {
        return Ok(true);
    }
    // Known dead, and the two depths disagree about it, which is why this
    // declines instead of guessing: `CalcClipLCD`'s shared tail (0x52A88,
    // reached by `b` from all four of its transform arms) returns 1 for a
    // non-positive `w`, so a zero return means `w > 0`. If it ever did not, the
    // 8-bit arms would draw nothing (`cmp ip,#0 / ble` at each row head) while
    // the 4-bit ones would still take their odd-`srcX` head and then find
    // `w-1 = -1` odd and draw a tail pixel too.
    if w <= 0 {
        return Ok(false);
    }

    // `GbmImage::GetColorBit` (0x8529C) is `ldrb [img]`; the low nibble is the
    // depth (+0x110 `and r0,r0,#0xf`).
    let four_bit = vm.mem.read_u8(img + gbm::COLOR_BIT) as u32 & 0xf == 4;
    // A 4-bit source can only ever form a nibble, so only 16 entries are read:
    // asking for the 8-bit 512 bytes would read past a short palette and turn
    // into a permanent decline. Same reasoning, same numbers, as the other two
    // image ports.
    let pal_entries = if four_bit { 16usize } else { 256usize };
    let mut pal = [0u16; 256];
    let pal_addr = vm.mem.read_u32(img + gbm::PALETTE);
    let Some(pal_bytes) = vm.mem.slice(pal_addr, pal_entries as u32 * 2) else {
        return Ok(false);
    };
    for (i, e) in pal.iter_mut().take(pal_entries).enumerate() {
        *e = u16::from_le_bytes([pal_bytes[i * 2], pal_bytes[i * 2 + 1]]);
    }
    let pixels = vm.mem.read_u32(img + gbm::PIXELS);

    // The destination pitch is `FrameBufferInfo::GetWidth()` — the width in
    // *pixels*. `GetBpl` is never called by this function, so a padded surface
    // would break here exactly as it breaks in the shipped code.
    let stride = fb.width;
    // The destination row and column, from the table above. `flip_y` picks the
    // corner the row walk starts from and `flip_x` the one the column walk
    // does; the steps then follow.
    let row_of = |r: i32| {
        let dy = if flip_y {
            y.wrapping_add(h).wrapping_sub(1).wrapping_sub(r)
        } else {
            y.wrapping_add(r)
        };
        dy.wrapping_mul(stride)
    };
    let col0 = if flip_x {
        x.wrapping_add(w).wrapping_sub(1)
    } else {
        x
    };
    let dstep: i32 = if flip_x { -1 } else { 1 };

    if !four_bit {
        // ---- 8-bit source: +0x14c / +0x3c0 / +0x460 / +0x4f0 --------------
        // +0x12c `mla r1,r5,r1,r2`: the first source byte is
        // `img_w*srcY + srcX`, and the row start walks by `img_w`.
        let mut src_row = img_w.wrapping_mul(sy).wrapping_add(sx);
        for r in 0..h {
            let row = row_of(r);
            for c in 0..w {
                // +0x1b0..+0x1c8 and its three twins: `pixels[src_row + c]`,
                // palette lookup as a half-word, then the skip test.
                let b = vm
                    .mem
                    .read_u8(pixels.wrapping_add(src_row.wrapping_add(c) as u32));
                let px = pal[b as usize];
                if px as u32 == skip {
                    continue;
                }
                let di = col0.wrapping_add(c.wrapping_mul(dstep));
                let at = fb
                    .base
                    .wrapping_add(row.wrapping_add(di).wrapping_mul(2) as u32);
                vm.mem.write_u16(at, px);
            }
            src_row = src_row.wrapping_add(img_w);
        }
        return Ok(true);
    }

    // ---- 4-bit source: +0x258 / +0x590 / +0x6f0 / +0x858 -----------------
    let row_bytes = if img_w & 1 != 0 {
        (img_w + 1) >> 1
    } else {
        img_w >> 1
    };
    let odd_sx = sx & 1 != 0;
    let mut src_row = row_bytes.wrapping_mul(sy).wrapping_add(sx >> 1);

    for r in 0..h {
        let row = row_of(r);
        let mut di = col0;
        let mut src = src_row;
        // The width the pair loop is given, reset every row.
        let mut wr = w;

        // The odd-`srcX` head. The destination pointer advances whether or not
        // the pixel was skipped (the `add` that bumps it is unconditional while
        // the store beside it is `strhne`), so a skipped pixel still moves the
        // column on.
        if odd_sx {
            let b = vm.mem.read_u8(pixels.wrapping_add(src as u32)) as usize;
            let px = pal[b & 0xf];
            if px as u32 != skip {
                let at = fb
                    .base
                    .wrapping_add(row.wrapping_add(di).wrapping_mul(2) as u32);
                vm.mem.write_u16(at, px);
            }
            di = di.wrapping_add(dstep);
            src = src.wrapping_add(1);
            wr -= 1;
        }

        // `asr r4,fp,#1` whole bytes, one source byte per destination pair,
        // high nibble first. Both skip branches land before the pointer bumps,
        // so a skipped pixel still advances.
        let pairs = wr >> 1;
        for _ in 0..pairs {
            let b = vm.mem.read_u8(pixels.wrapping_add(src as u32)) as usize;
            for (n, px) in [(0i32, pal[b >> 4]), (1, pal[b & 0xf])] {
                if px as u32 == skip {
                    continue;
                }
                let at = fb.base.wrapping_add(
                    row.wrapping_add(di.wrapping_add(n.wrapping_mul(dstep)))
                        .wrapping_mul(2) as u32,
                );
                vm.mem.write_u16(at, px);
            }
            di = di.wrapping_add(dstep.wrapping_mul(2));
            src = src.wrapping_add(1);
        }

        // An odd remaining width leaves one pixel, taken from the **high**
        // nibble of the byte the pair loop stopped on. Dropping it loses one
        // column of every odd-width sprite.
        if wr & 1 != 0 {
            let b = vm.mem.read_u8(pixels.wrapping_add(src as u32)) as usize;
            let px = pal[b >> 4];
            if px as u32 != skip {
                let at = fb
                    .base
                    .wrapping_add(row.wrapping_add(di).wrapping_mul(2) as u32);
                vm.mem.write_u16(at, px);
            }
        }

        // The source row start walks by the packed pitch; the odd-`srcX` head's
        // extra byte is applied to a working copy and deliberately does not
        // compound across rows.
        src_row = src_row.wrapping_add(row_bytes);
    }
    Ok(true)
}

// ---- the frosted-glass background ------------------------------------------

/// `Graphic::BoxBlur565(fb, k)` (0x55520) — the stall when a menu opens.
///
/// Every caller is a screen being built over a frozen picture of the one
/// before: `StateInGameMenu`'s constructors (the in-game menu button) and
/// `Process`, `StateInExpertMenu`'s, `StateMainMenu::KeyTitle` (the tap that
/// leaves the title) and `StateMainMenu::Process`. All sixteen pass
/// `Graphic::GetFB()` and `k = 2`, through `Graphic::BoxBlur` (0x557A8), which
/// tail-branches here when `this[0x5c] == 1` — so `lr` is still the original
/// caller's, as [`Vm::run`]'s return needs. It runs once per opening, and that
/// once is the whole frame:
///
/// ```text
///   menu button, --profile-frames on that frame:
///     20.6 M guest instructions (a gameplay frame is ~0.3 M)
///     73.4% BoxBlur565  23.6% __divsi3           = 97% of the frame
///   the title tap (frame 794 of ingame.txt): 19.5 M, the same two
/// ```
///
/// — about 0.6 s on a Galaxy S9, with the screen frozen, and more on the phone
/// itself: the cost is per pixel, and at the shell's 585x270 canvas the blur
/// alone is 31.6 M (~0.9 s). It is expensive for
/// no reason the output needs: for every pixel it re-walks the whole box and
/// then makes **three `__divsi3` calls**, on a core with no divide instruction,
/// by a count that varies because samples outside the surface are skipped
/// rather than clamped.
///
/// The box for output `(x, y)` is columns `x-1 ..= x+k-2` by rows
/// `y-1 ..= y+k-2` — it starts one pixel up and left (`mvn fp,#0` and
/// `[sp,#0x14] = -1`, stepped with the output), clipped to `[0,w) x [0,h)`
/// by the two `movge/movlt` pairs at 0x5565C and 0x55688. Each channel's sum
/// is divided by the number of samples that survived, truncating, and packed
/// as `(b & 0x1f) | (r << 11) | ((g << 5) & 0x7e0)` (0x55704..0x5573C — red is
/// not masked, and needs none: a mean of 5-bit values is 5 bits).
///
/// The host keeps all of that — same box, same variable divisor, same
/// truncation, same packing — but carries column sums down the rows and a row
/// sum across, so each output is O(1) instead of O(k²) plus three divides.
/// Every intermediate is an exact integer, so the result is identical pixel
/// for pixel (`box_blur_matches_the_guest_loop` checks it against a
/// transcription of the shipped loop).
///
/// The source is read and the destination written **packed at the source
/// width** (`r2 += 2` per pixel and `+= 2*w` per row, 0x55740 / 0x55778):
/// neither buffer's own stride is consulted. The destination is the `Graphic`
/// object's other frame buffer, `this[8]` (0x55548) — the off-screen surface
/// `onStartApp` creates at the canvas size — and nothing is copied back; the
/// menu draws that surface as its backdrop.
fn box_blur565(vm: &mut Vm) -> Result<bool, String> {
    let this = vm.arg(0);
    let src_fbp = vm.arg(1);
    let k = vm.arg(2) as i32;
    let dst_fbp = vm.mem.read_u32(this.wrapping_add(8));
    if src_fbp == 0 || dst_fbp == 0 {
        return Ok(false);
    }
    let src = FbInfo::read(vm, src_fbp);
    let dst = FbInfo::read(vm, dst_fbp);
    // Both `FrameBufferInfo` constructors divide the stride by the width
    // (0x52010) before anything else happens; a zero width is the guest's
    // divide-by-zero, not ours to reproduce.
    if src.width == 0 || dst.width == 0 {
        return Ok(false);
    }
    let (w, h) = (src.width, src.height);
    // `cmp r2,#0 / ble` at 0x55584 and `cmp r5,#0 / ble` at 0x555D4: an empty
    // surface draws nothing, and that is a completed call.
    if w <= 0 || h <= 0 {
        return Ok(true);
    }
    // With `k >= 2` the box always holds `(x, y)` itself, so no count is zero.
    // `k == 1` samples only `(x-1, y-1)` — nothing on the first row and column
    // — and `k <= 0` samples nothing at all; both would divide by zero in the
    // guest. Every caller passes 2. The upper bound keeps the sums far inside
    // `u32` (63·k² per channel).
    if !(2..=1024).contains(&k) {
        return Ok(false);
    }
    let Some(bytes) = (w as u32).checked_mul(h as u32).and_then(|n| n.checked_mul(2)) else {
        return Ok(false);
    };
    // The guest interleaves reads of the source with writes of the
    // destination. Computing every output from a snapshot is only the same
    // thing if the two do not overlap; in-place, later boxes would see earlier
    // outputs. They are different surfaces here (`this[0]` and `this[8]`), so
    // this never declines in practice — but a silent difference would leave no
    // trace in the frames an A/B happens to dump.
    if src.base < dst.base.wrapping_add(bytes) && dst.base < src.base.wrapping_add(bytes) {
        return Ok(false);
    }
    let Some(pixels) = vm.mem.slice(src.base, bytes) else { return Ok(false) };
    let pixels: Vec<u16> =
        pixels.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let out = box_blur565_pixels(&pixels, w as usize, h as usize, k as usize);
    let Some(dst_buf) = vm.mem.slice_mut(dst.base, bytes) else { return Ok(false) };
    dst_buf.copy_from_slice(&out);
    Ok(true)
}

/// The arithmetic of [`box_blur565`], on host memory: `src` is `w*h` pixels
/// packed at width `w`, and the result is the same `w*h` pixels as
/// little-endian bytes. `k >= 2`.
fn box_blur565_pixels(src: &[u16], w: usize, h: usize, k: usize) -> Vec<u8> {
    debug_assert!(k >= 2 && src.len() == w * h);
    let mut out = vec![0u8; w * h * 2];
    // Per-column sums over the current row window `[row_lo, row_hi)`.
    let (mut cr, mut cg, mut cb) = (vec![0u32; w], vec![0u32; w], vec![0u32; w]);
    let (mut row_lo, mut row_hi) = (0usize, 0usize);
    for y in 0..h {
        // Rows `y-1 ..= y+k-2`, clipped: `[max(0, y-1), min(h, y+k-1))`.
        let (want_lo, want_hi) = (y.saturating_sub(1), (y + k - 1).min(h));
        while row_hi < want_hi {
            for (x, &p) in src[row_hi * w..row_hi * w + w].iter().enumerate() {
                let p = p as u32;
                cr[x] += p >> 11;
                cg[x] += (p & 0x7e0) >> 5;
                cb[x] += p & 0x1f;
            }
            row_hi += 1;
        }
        while row_lo < want_lo {
            for (x, &p) in src[row_lo * w..row_lo * w + w].iter().enumerate() {
                let p = p as u32;
                cr[x] -= p >> 11;
                cg[x] -= (p & 0x7e0) >> 5;
                cb[x] -= p & 0x1f;
            }
            row_lo += 1;
        }
        let rows = (want_hi - want_lo) as u32;

        // The same window across: columns `[max(0, x-1), min(w, x+k-1))`.
        let (mut sr, mut sg, mut sb) = (0u32, 0u32, 0u32);
        let (mut col_lo, mut col_hi) = (0usize, 0usize);
        for x in 0..w {
            let (want_lo, want_hi) = (x.saturating_sub(1), (x + k - 1).min(w));
            while col_hi < want_hi {
                sr += cr[col_hi];
                sg += cg[col_hi];
                sb += cb[col_hi];
                col_hi += 1;
            }
            while col_lo < want_lo {
                sr -= cr[col_lo];
                sg -= cg[col_lo];
                sb -= cb[col_lo];
                col_lo += 1;
            }
            let n = rows * (want_hi - want_lo) as u32;
            // 0x556FC: blue first, then red, then green — the order does not
            // matter to the value, only the masks do.
            let v = ((sb / n) & 0x1f) | ((sr / n) << 11) | (((sg / n) << 5) & 0x7e0);
            let o = (y * w + x) * 2;
            out[o..o + 2].copy_from_slice(&(v as u16).to_le_bytes());
        }
    }
    out
}

// ---- the font pixel writer ---------------------------------------------------
//
// The main menu is 79% font stack and this one function is half of it. Unlike
// every other port in this file it touches no `Graphic` object, no `GbmImage`
// and no `MC_GrpContext`: `_midas_funcFntDrawPixelPreRGB` takes no frame-buffer
// argument at all. Its geometry, its clip and its palette are seven `_midas_*`
// globals that `MX_fntDrawString` fills in just before it enters, so none of
// this module's other helpers apply — `FbInfo`, `blend_table565`, `gbm` and
// `ctx` are all the wrong tools here and two of them would not even compile.

/// Byte offsets inside `_midas_internalFntClip` (16 bytes of `.bss`): a
/// half-open box `[x0,x1) x [y0,y1)`, the same sense as [`ctx`]'s clip and
/// tested in that order at `PreRGB+0x80..+0xb4`.
///
/// `MX_fntDrawString+0x40..+0x5c` fills it with a full-screen box when the
/// caller passed no `MC_GrpContext`, and hands its address to
/// `MC_grpGetContext` at `+0x110` to be filled from a real one when the caller
/// did. Nothing else in the image writes it.
mod fntclip {
    pub const X0: u32 = 0x00;
    pub const Y0: u32 = 0x04;
    pub const X1: u32 = 0x08;
    pub const Y1: u32 = 0x0c;
}

/// Addresses the font-pixel port reads, resolved once at
/// [`Vm::install_patches`] time rather than per pixel.
///
/// Thirteen `HashMap<String, u32>` lookups per *pixel* — 3.25 M pixels in the
/// ingame replay — would spend most of what the port saves. These are symbol
/// addresses: they cannot move while a process lives, so one resolution is
/// enough, and it lives on the `Vm` rather than in a `static` so a second image
/// in the same process could not silently inherit the first one's addresses.
#[derive(Clone, Copy)]
pub(crate) struct FntGlobals {
    draw_dir: u32,
    lcd: u32,
    lcd_w: u32,
    lcd_h: u32,
    /// `_midas_lcdRGB` — **bytes** per pixel, not a format enum: 2 for RGB565,
    /// 4 otherwise. `MX_fntDrawString+0xc4` writes 2 when `fb[0x0c] == 0x10`.
    lcd_rgb: u32,
    clip: u32,
    /// `_midas_internalFntOpacity`, 16x16x16 bytes, built once by
    /// `MX_fntInit+0x38..+0x88` as `(src*(op+1) + dst*(15-op)) >> 4`.
    opacity: u32,
    /// The *source* colour, three ints. `PreRGB` overwrites these — see below.
    rgb0: [u32; 3],
    /// The *destination* colour, three ints.
    rgb1: [u32; 3],
    /// `MIDASKernelManager::instance` — the singleton pointer, not the object.
    kernel: u32,
}

impl FntGlobals {
    /// `None` when any of the thirteen is missing, which makes the port decline
    /// rather than read a wrong address.
    pub(crate) fn resolve(vm: &Vm) -> Option<Self> {
        Some(Self {
            draw_dir: vm.symbol("_midas_drawDIR")?,
            lcd: vm.symbol("_midas_lcd")?,
            lcd_w: vm.symbol("_midas_lcdWidth")?,
            lcd_h: vm.symbol("_midas_lcdHeight")?,
            lcd_rgb: vm.symbol("_midas_lcdRGB")?,
            clip: vm.symbol("_midas_internalFntClip")?,
            opacity: vm.symbol("_midas_internalFntOpacity")?,
            rgb0: [
                vm.symbol("_midas_rgb_r0")?,
                vm.symbol("_midas_rgb_g0")?,
                vm.symbol("_midas_rgb_b0")?,
            ],
            rgb1: [
                vm.symbol("_midas_rgb_r1")?,
                vm.symbol("_midas_rgb_g1")?,
                vm.symbol("_midas_rgb_b1")?,
            ],
            kernel: vm.symbol("_ZN18MIDASKernelManager8instanceE")?,
        })
    }
}

/// `kernel[0x4F4] == 0x10` — the pixel-layout selector both RGB helpers branch
/// on (`MC_grpGetRGBFromPixel+0x1C`, `MC_grpGetPixelFromRGB+0x18`).
///
/// The chain in front of it is `MIDASKernelManager::getInstance()`, one load of
/// the singleton pointer, then `getKernel()`, a bare `bx lr` handing the same
/// pointer back. `setWIPIDisplayInfo` writes 0x10 for the 565 layout.
fn kernel_565(vm: &Vm, g: &FntGlobals) -> bool {
    let kernel = vm.mem.read_u32(g.kernel);
    vm.mem.read_u32(kernel.wrapping_add(0x4f4)) == 0x10
}

/// `MC_grpGetRGBFromPixel+0x70` — the 565 decompose, inlined.
///
/// The guest's shifts are `asr`, but every result is masked back into a byte,
/// so the sign cannot reach the answer and these logical shifts are exact.
fn rgb_from_565(p: u32) -> (u32, u32, u32) {
    ((p >> 8) & 0xf8, (p >> 3) & 0xfc, (p << 3) & 0xff)
}

/// `MC_grpGetPixelFromRGB+0x50` — the 565 compose, inlined.
///
/// **Unmasked, exactly as the guest leaves it** (`orr r4, r5, r4, lsl #5`;
/// `orr r0, r4, r6, lsl #11`). The truncation happens at the store: the guest
/// spills the whole word to `[sp,#0x14]` and then `memcpy`s `_midas_lcdRGB` = 2
/// bytes of it, so only the low half-word reaches the screen.
fn pixel_from_565(r: u32, g: u32, b: u32) -> u32 {
    ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)
}

/// `_midas_funcFntDrawPixel` (0x39A58, 100 B) plus the body of
/// `_midas_funcFntDrawPixelPreRGB` (0x39798, 704 B) it tail-branches into —
/// **one font pixel**, and about half of the main menu's cost.
///
/// `(int x, int y, int pixel, int opacity)`, all four in r0..r3, nothing on the
/// stack. Argument roles are settled by the wrappers rather than guessed:
/// `_midas_funcFntDrawPixelW` varies **r0** by ±1 and `…H` varies **r1**.
///
/// # Why a hundred instructions are worth porting
///
/// Because it runs once per glyph *pixel* — 234 K times in the new-game replay,
/// 3.25 M in the ingame one — and because **the only store in the whole
/// function is `memcpy@plt`** (`PreRGB+0xe8`). Every font pixel is an HLE round
/// trip; roughly half the win is that call rather than the arithmetic.
///
/// The measurement that put it on the list is per-scene (ANALYSIS.md §31.4):
/// the main menu is 79% font stack, and the S25 measurement in §32.1 showed why
/// that matters — a static menu the player is just looking at still costs 24
/// full re-renders a second.
///
/// # Where it draws
///
/// `MX_fntDrawString+0xb8..+0xc4` caches the frame buffer into four globals
/// just before entering: `_midas_lcdWidth` = `fb[0x00]`, `_midas_lcdHeight` =
/// `fb[0x04]`, `_midas_lcd` = `fb[0x10]`, and `_midas_lcdRGB` = 2 if
/// `fb[0x0c] == 0x10` else 4. So **the row pitch is the width in pixels**, and
/// the base is a copy — this port must not go looking for a `FrameBufferInfo`.
///
/// # What it declines, and why the list is longer than the guest's
///
/// * `*_midas_lcdRGB != 2` (`PreRGB+0x124`) — the 4-bytes-per-pixel body.
///   `nativeInitDisplay` selects 565, so 2 is what runs.
/// * `*_midas_drawDIR != 0`. This is **not** a guest branch, it is a
///   deliberately narrower port. `PreRGB+0x24..+0x30` is a three-way dispatch
///   that remaps the axes for a rotated IME keyboard, and `imeLocal_QWERTY_Paint`
///   really does set the global (+0x9c) and clear it (+0xc8) while drawing its
///   own labels straight down this chain. But **no replay in `tools_re/scripts`
///   opens the IME**, so the two remapping arms have no acceptance test at all —
///   and untested transcription is exactly what this file's contract exists to
///   prevent. Declining runs the shipped code for them, which is free and
///   correct. Lifting this needs a replay that opens the IME first.
/// * `opacity < 0` (`PreRGB+0xb8` is a *signed* `cmp #0xfe`). A negative
///   opacity takes the blend and then `asr r3,r3,#4` indexes the table from
///   before its start. Nothing observed passes one; the shipped code can keep
///   that case.
/// * `kernel[0x4F4] != 0x10` — the two RGB helpers' own selector.
/// * a source component outside 0..0xff. The table read at `PreRGB+0x1a4` is an
///   `ldrb` with **no bound**, indexed `op4*256 + (src>>4)*16 + (dst>>4)`;
///   `op4` and `dst>>4` are 0..15 by construction, so only a source component
///   that is not a byte can leave the 4,096-byte table. `_midas_funcFntGroup_Draw`
///   writes those three words directly, so the guard is real rather than
///   defensive.
///
/// **`_midas_funcFntDrawPixelPreRGB` is deliberately *not* in [`PATCHES`].**
/// Its only other caller is `_midas_funcFntGroup_Draw+0x1cc`, which is entered
/// zero times in all five replay scenes — so patching that entry would ship
/// code with no acceptance test to buy nothing measurable. Every call the game
/// actually makes arrives at `_midas_funcFntDrawPixel`.
///
/// # Three quirks a tidied-up port would lose
///
/// * **It is not pure.** `PreRGB+0x224/+0x238/+0x23c` store the *blended*
///   component back over `_midas_rgb_b0`/`_midas_rgb_r0`/`_midas_rgb_g0` — the
///   same globals that carried the source colour in. Drop those and every pixel
///   after the first is wrong.
/// * **There is no `opacity == 0` early out.** Opacity 0 still goes through the
///   table, and `table[0][s][d] = (s + 15*d) >> 4` is not `d`.
/// * **The blend loses a bit per channel.** The table holds 4-bit values, the
///   code scales them by 16 and the compose divides by 8/4/8. That is the
///   shipped output.
fn fnt_draw_pixel(vm: &mut Vm) -> Result<bool, String> {
    let Some(g) = vm.fnt else { return Ok(false) };

    // ---- decline before any store ----------------------------------------
    // The guest reads these later, after its clip tests, so declining here
    // also hands back calls that would have been clipped out. Same result, one
    // interpreted no-op.
    if vm.mem.read_u32(g.lcd_rgb) != 2 {
        return Ok(false);
    }
    if vm.mem.read_u32(g.draw_dir) != 0 {
        return Ok(false);
    }
    let opacity = vm.arg(3) as i32;
    if opacity < 0 {
        return Ok(false);
    }
    if !kernel_565(vm, &g) {
        return Ok(false);
    }

    let x = vm.arg(0) as i32;
    let y = vm.arg(1) as i32;
    let pixel = vm.arg(2);

    // `_midas_funcFntDrawPixel+0x38`: `MC_grpGetRGBFromPixel(pixel,
    // &_midas_rgb_r0, &_midas_rgb_g0, &_midas_rgb_b0)`. **It decomposes the
    // pixel; it does not load the globals.** Porting only the loads would draw
    // every glyph in the previous glyph's colour, because the blend below
    // leaves its own output in those three words. These stores happen before
    // the clip tests, exactly as they do in the guest — and they are the same
    // three values the shipped function would write, so a later decline (there
    // is none after this point) would still be idempotent.
    let (sr, sg, sb) = rgb_from_565(pixel);
    vm.mem.write_u32(g.rgb0[0], sr);
    vm.mem.write_u32(g.rgb0[1], sg);
    vm.mem.write_u32(g.rgb0[2], sb);

    // ---- PreRGB+0x4c..+0xb4: eight signed tests, and nothing is written ----
    // There is no opacity test among them: a fully transparent pixel is still
    // clipped, indexed and blended.
    let lcd_w = vm.mem.read_u32(g.lcd_w) as i32;
    let lcd_h = vm.mem.read_u32(g.lcd_h) as i32;
    if y < 0 || x < 0 || x >= lcd_w || y >= lcd_h {
        return Ok(true);
    }
    if x < vm.mem.read_u32(g.clip + fntclip::X0) as i32
        || y < vm.mem.read_u32(g.clip + fntclip::Y0) as i32
        || x >= vm.mem.read_u32(g.clip + fntclip::X1) as i32
        || y >= vm.mem.read_u32(g.clip + fntclip::Y1) as i32
    {
        return Ok(true);
    }

    // `PreRGB+0xd0 mla r5, r2, r5, r6` then `+0xdc mla r0, r3, r5, r0`, both
    // 32-bit wrapping: `dst = lcd + lcdRGB * (lcdWidth * y + x)`, and lcdRGB is
    // 2 on the ported path.
    let idx = (lcd_w as u32).wrapping_mul(y as u32).wrapping_add(x as u32);
    let dst = vm.mem.read_u32(g.lcd).wrapping_add(idx.wrapping_mul(2));

    // ---- PreRGB+0xb8: opaque ---------------------------------------------
    // The `pixel` argument was spilled whole at +0x0c and the `memcpy` at +0xe8
    // copies `_midas_lcdRGB` = 2 bytes of it, little end first.
    if opacity > 0xfe {
        vm.mem.write_u16(dst, pixel as u16);
        return Ok(true);
    }

    // ---- PreRGB+0x110: the blend ------------------------------------------
    // The destination half-word, read at the address the store will use
    // (+0x27c `ldrh r0,[r1,r3]`), then decomposed into `_midas_rgb_{r,g,b}1`
    // through the same helper (+0x168). Nothing else in the image reads those
    // three words, but the stores are three words and this is a port.
    let dpx = vm.mem.read_u16(dst) as u32;
    let (dr, dg, db) = rgb_from_565(dpx);
    vm.mem.write_u32(g.rgb1[0], dr);
    vm.mem.write_u32(g.rgb1[1], dg);
    vm.mem.write_u32(g.rgb1[2], db);

    // `_midas_internalFntOpacity` is **read from guest memory**, never
    // recomputed: if `MX_fntInit` has not run it is all zeros and the shipped
    // code draws black, which this then also does.
    let op4 = (opacity >> 4) as u32; // +0x114 `asr r3,r3,#4`, 0..15
    let entry = |s: u32, d: u32| (((op4 << 4) + (s >> 4)) << 4) + (d >> 4);
    let tr = vm.mem.read_u8(g.opacity.wrapping_add(entry(sr, dr))) as u32;
    let tg = vm.mem.read_u8(g.opacity.wrapping_add(entry(sg, dg))) as u32;
    let tb = vm.mem.read_u8(g.opacity.wrapping_add(entry(sb, db))) as u32;

    // +0x21c..+0x228 scale the 4-bit table byte back to eight bits, and
    // +0x224/+0x238/+0x23c store it back over the source colour — blue first,
    // then red, then green. This is the quirk: the function is not pure.
    let (nr, ng, nb) = (tr << 4, tg << 4, tb << 4);
    vm.mem.write_u32(g.rgb0[2], nb);
    vm.mem.write_u32(g.rgb0[0], nr);
    vm.mem.write_u32(g.rgb0[1], ng);

    // +0x240 `MC_grpGetPixelFromRGB(r, g, b)`, +0x248 the spill, then the same
    // `memcpy` the opaque path uses: the low half-word only.
    vm.mem.write_u16(dst, pixel_from_565(nr, ng, nb) as u16);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Graphic::BoxBlur565`'s loop as the guest runs it (0x555D4..0x55754):
    /// re-walk the whole box per output, skip samples outside the surface,
    /// divide by however many survived.
    fn guest_box_blur(src: &[u16], w: i32, h: i32, k: i32) -> Vec<u8> {
        let mut out = Vec::with_capacity((w * h * 2) as usize);
        for y in 0..h {
            for x in 0..w {
                let (mut r, mut g, mut b, mut n) = (0i32, 0i32, 0i32, 0i32);
                for yy in (y - 1)..(y - 1 + k) {
                    if !(0..h).contains(&yy) {
                        continue;
                    }
                    for xx in (x - 1)..(x - 1 + k) {
                        if !(0..w).contains(&xx) {
                            continue;
                        }
                        let p = src[(yy * w + xx) as usize] as i32;
                        n += 1;
                        b += p & 0x1f;
                        g += (p & 0x7e0) >> 5;
                        r += p >> 11;
                    }
                }
                let v = ((b / n) & 0x1f) | ((r / n) << 11) | (((g / n) << 5) & 0x7e0);
                out.extend_from_slice(&(v as u16).to_le_bytes());
            }
        }
        out
    }

    #[test]
    fn box_blur_matches_the_guest_loop() {
        // A deterministic, full-range pattern: every channel value occurs,
        // including 31/63/31, so a sum or a mask that is off shows.
        let mut seed = 0x1234_5678u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (seed >> 8) as u16
        };
        for &(w, h) in &[(1, 1), (1, 7), (7, 1), (2, 2), (13, 9), (40, 24), (61, 27)] {
            let src: Vec<u16> = (0..w * h).map(|_| next()).collect();
            for k in 2..=6 {
                assert_eq!(
                    box_blur565_pixels(&src, w as usize, h as usize, k as usize),
                    guest_box_blur(&src, w, h, k),
                    "{w}x{h} k={k}"
                );
            }
        }
        // The saturated corner: all-white stays all-white at every count.
        let white = vec![0xffffu16; 5 * 4];
        assert!(box_blur565_pixels(&white, 5, 4, 2).chunks(2).all(|c| c == [0xff, 0xff]));
    }
}
