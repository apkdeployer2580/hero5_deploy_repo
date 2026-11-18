//! GLES 1.x as a watched seam, not a GPU.
//!
//! The guest's GL usage was measured before any of this was written
//! (ANALYSIS.md): all three `glDrawArrays` sites live in
//! `Texture2D::update()`, the single `glTexImage2D` in
//! `Texture2D::createTexture()`, the single `glTexSubImage2D` in
//! `Texture2D::bindTextureFromDirect()`. The game renders into its own
//! framebuffer with the `Graphic` blitter classes and uses GL only to put
//! that buffer on screen as one textured quad. So this module is a state
//! machine: it records every call, keeps texture pixels host-side, and on
//! `glDrawArrays` scale-copies the bound texture into a host framebuffer on
//! the CPU. No GL library is involved anywhere — the same shape that shipped
//! in the sibling chronosSword (`cs_core/src/gl.rs`).
//!
//! A draw the state machine cannot map (unexpected vertex format, no texture,
//! no ortho) is recorded in the log with its raw parameters rather than
//! guessed at — the first real frame tells us the exact format the guest
//! uses, which beats speculating now.

use std::collections::{HashMap, HashSet};

use crate::Vm;

// The GLES 1.x enums this guest can reach through its 19 imports.
const GL_MODELVIEW: u32 = 0x1700;
const GL_PROJECTION: u32 = 0x1701;
const GL_VERTEX_ARRAY: u32 = 0x8074;
const GL_TEXTURE_COORD_ARRAY: u32 = 0x8078;
const GL_BYTE: u32 = 0x1400;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_SHORT: u32 = 0x1402;
const GL_FLOAT: u32 = 0x1406;
const GL_FIXED: u32 = 0x140C;
const GL_RGB: u32 = 0x1907;
const GL_RGBA: u32 = 0x1908;
const GL_LUMINANCE: u32 = 0x1909;
const GL_LUMINANCE_ALPHA: u32 = 0x190A;
const GL_UNSIGNED_SHORT_4_4_4_4: u32 = 0x8033;
const GL_UNSIGNED_SHORT_5_5_5_1: u32 = 0x8034;
const GL_UNSIGNED_SHORT_5_6_5: u32 = 0x8363;

#[derive(Clone)]
pub struct Pointer {
    pub size: u32,
    pub ty: u32,
    pub stride: u32,
    pub addr: u32,
}

pub struct Texture {
    pub w: i32,
    pub h: i32,
    pub format: u32,
    pub ty: u32,
    /// Raw bytes exactly as uploaded; decoded per-texel at blit time.
    pub pixels: Vec<u8>,
}

/// Every input the pixel loop in [`Vm::gl_draw_body`] reads, reduced to
/// something comparable.
///
/// The loop writes `fb` and nothing else in this crate writes `fb`, so if all
/// of these are what they were last draw, `fb` already holds the answer and
/// the copy is pure waste. That case is not rare — it is the *common* case:
/// `updateScreen` issues a `glDrawArrays` on every `nativeLoop` while the
/// guest only redraws on its own 36 ms timer, so at a 2 ms poll grid roughly
/// nine draws in ten repeat the previous one.
///
/// Texture *contents* are covered by `tex_gen` rather than by hashing the
/// pixels: this crate keeps its own copy of every texture
/// ([`Texture::pixels`]) and the only writers are `glTexImage2D` and
/// `glTexSubImage2D`, so a counter bumped there is exact and free. Vertex and
/// texcoord arrays live in *guest* memory and can change with no GL call at
/// all, which is why the geometry enters the key as the resolved rectangle
/// and texcoord bounds — already computed by the time the key is built — and
/// not as the pointers they came from.
#[derive(PartialEq)]
struct DrawKey {
    bound: u32,
    tex_gen: u64,
    rect: (i32, i32, i32, i32),
    /// `(u0, v0, u1, v1)` as bits, or `None` for the untextured fill path.
    /// Bits, not floats, so `PartialEq` is total.
    tex: Option<[u32; 4]>,
    fill: u32,
    fb: (i32, i32),
}

pub struct GlState {
    next_name: u32,
    pub bound: u32,
    pub textures: HashMap<u32, Texture>,
    pub caps: HashSet<u32>,
    pub client: HashSet<u32>,
    pub matrix_mode: u32,
    /// `glOrthof(l, r, b, t, n, f)` on the projection stack, if one was set
    /// since the last `glLoadIdentity`.
    pub ortho: Option<[f32; 6]>,
    pub viewport: (i32, i32, i32, i32),
    pub color: [f32; 4],
    pub vertex: Option<Pointer>,
    pub texcoord: Option<Pointer>,
    /// Host framebuffer, `0x00RRGGBB`, row-major, top-down.
    pub fb: Vec<u32>,
    pub fb_w: i32,
    pub fb_h: i32,
    pub draws: u64,
    /// Bumped by every texture upload, so a draw can tell whether the pixels
    /// it is about to copy are the ones it copied last time.
    tex_gen: u64,
    /// Everything the pixel loop below reads, from the previous draw. See
    /// [`Vm::gl_draw_body`].
    last_draw: Option<DrawKey>,
    /// Bumped whenever the pixel loop actually runs — i.e. whenever `fb` can
    /// have changed.
    ///
    /// The shell keys its present on this rather than on [`GlState::draws`]:
    /// the guest issues a `glDrawArrays` on every poll, so `draws` says
    /// "the guest was offered a tick", while this says "the picture the shell
    /// would put up is not the one already up".
    pub fb_gen: u64,
    /// Draws whose output was already in `fb`, for the periodic line.
    pub draws_elided: u64,
    /// Wall time spent in [`Vm::gl_draw`] since the last reader reset it.
    ///
    /// This is the host's own upscale-and-sample pass, not the guest's
    /// drawing, and it runs **once per `nativeLoop`** whether or not the
    /// picture changed: `updateScreen` (0x1520C4) calls `Texture2D::update`
    /// (0x153138) unconditionally and every path through it ends in
    /// `glDrawArrays` (ANALYSIS.md §31.9). So it is a per-poll cost, which is
    /// exactly what has to be known before the shell polls more often — and
    /// it is invisible in `t_present`, which only counts frames that reached
    /// the screen.
    pub draw_time: std::time::Duration,
}

impl GlState {
    /// Forget the last draw, so the next one rebuilds `fb` from the texture
    /// instead of eliding as "the same picture".
    ///
    /// Needed by anything the host paints *on top* of the guest's picture: the
    /// elision key describes the guest's draw only, so a host overlay that
    /// appears or disappears while the guest's picture is unchanged would
    /// otherwise not reach the screen — and one that moved would leave its old
    /// copy behind, because only the pixel loop can restore the background.
    pub fn forget_last_draw(&mut self) {
        self.last_draw = None;
    }

    pub fn new(w: i32, h: i32) -> Self {
        Self {
            next_name: 1,
            bound: 0,
            textures: HashMap::new(),
            caps: HashSet::new(),
            client: HashSet::new(),
            matrix_mode: GL_MODELVIEW,
            ortho: None,
            viewport: (0, 0, w, h),
            color: [1.0; 4],
            vertex: None,
            texcoord: None,
            fb: vec![0; (w.max(1) * h.max(1)) as usize],
            fb_w: w.max(1),
            fb_h: h.max(1),
            draws: 0,
            tex_gen: 0,
            last_draw: None,
            fb_gen: 0,
            draws_elided: 0,
            draw_time: std::time::Duration::ZERO,
        }
    }
}

fn bytes_per_texel(format: u32, ty: u32) -> usize {
    match ty {
        GL_UNSIGNED_SHORT_5_6_5 | GL_UNSIGNED_SHORT_4_4_4_4 | GL_UNSIGNED_SHORT_5_5_5_1 => 2,
        GL_UNSIGNED_BYTE => match format {
            GL_RGBA => 4,
            GL_RGB => 3,
            GL_LUMINANCE_ALPHA => 2,
            GL_LUMINANCE => 1,
            _ => 4,
        },
        _ => 2,
    }
}

/// Decode one texel to `0x00RRGGBB`. Low bits are replicated on expansion so
/// full-scale stays full-scale (0x1f -> 0xff, not 0xf8).
fn texel(px: &[u8], format: u32, ty: u32) -> u32 {
    match ty {
        GL_UNSIGNED_SHORT_5_6_5 => {
            let v = u16::from_le_bytes([px[0], px[1]]) as u32;
            let r = (v >> 11) & 0x1f;
            let g = (v >> 5) & 0x3f;
            let b = v & 0x1f;
            ((r << 3 | r >> 2) << 16) | ((g << 2 | g >> 4) << 8) | (b << 3 | b >> 2)
        }
        GL_UNSIGNED_SHORT_5_5_5_1 => {
            let v = u16::from_le_bytes([px[0], px[1]]) as u32;
            let r = (v >> 11) & 0x1f;
            let g = (v >> 6) & 0x1f;
            let b = (v >> 1) & 0x1f;
            ((r << 3 | r >> 2) << 16) | ((g << 3 | g >> 2) << 8) | (b << 3 | b >> 2)
        }
        GL_UNSIGNED_SHORT_4_4_4_4 => {
            let v = u16::from_le_bytes([px[0], px[1]]) as u32;
            let r = (v >> 12) & 0xf;
            let g = (v >> 8) & 0xf;
            let b = (v >> 4) & 0xf;
            (r * 0x11 << 16) | (g * 0x11 << 8) | b * 0x11
        }
        GL_UNSIGNED_BYTE => match format {
            GL_RGBA | GL_RGB => ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | px[2] as u32,
            GL_LUMINANCE | GL_LUMINANCE_ALPHA => {
                let l = px[0] as u32;
                l << 16 | l << 8 | l
            }
            _ => 0,
        },
        _ => 0,
    }
}

impl Vm {
    fn argf(&mut self, n: usize) -> f32 {
        f32::from_bits(self.arg(n))
    }

    /// Read `count` 2D positions from a client array. Components beyond x,y
    /// (a `size`-3 z) are skipped via the stride.
    fn read_attrib(&mut self, p: &Pointer, first: u32, count: u32) -> Option<Vec<[f32; 2]>> {
        let comp = match p.ty {
            GL_FLOAT | GL_FIXED => 4u32,
            GL_SHORT => 2,
            GL_BYTE | GL_UNSIGNED_BYTE => 1,
            _ => return None,
        };
        let stride = if p.stride != 0 { p.stride } else { p.size * comp };
        let mut out = Vec::with_capacity(count as usize);
        for i in first..first + count {
            let base = p.addr + i * stride;
            let mut v = [0f32; 2];
            for (c, slot) in v.iter_mut().enumerate() {
                let a = base + c as u32 * comp;
                let raw = self.mem.slice(a, comp)?;
                *slot = match p.ty {
                    GL_FLOAT => f32::from_le_bytes(raw.try_into().ok()?),
                    GL_FIXED => {
                        i32::from_le_bytes(raw.try_into().ok()?) as f32 / 65536.0
                    }
                    GL_SHORT => i16::from_le_bytes(raw.try_into().ok()?) as f32,
                    GL_BYTE => raw[0] as i8 as f32,
                    _ => raw[0] as f32,
                };
            }
            out.push(v);
        }
        Some(out)
    }

    /// Map a guest vertex to framebuffer pixels: ortho -> NDC -> viewport,
    /// then flip to top-down. With no ortho recorded the coordinates are
    /// taken as framebuffer pixels directly.
    fn to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        let (vx, vy, vw, vh) = self.gl.viewport;
        match self.gl.ortho {
            Some([l, r, b, t, ..]) => {
                let u = if r != l { (x - l) / (r - l) } else { 0.0 };
                let v_bottom = if t != b { (y - b) / (t - b) } else { 0.0 };
                let sx = vx as f32 + u * vw as f32;
                let sy_gl = vy as f32 + v_bottom * vh as f32;
                (sx, self.gl.fb_h as f32 - sy_gl)
            }
            None => (x, y),
        }
    }

    /// Timing shell around [`Vm::gl_draw_body`]. Separate so the body keeps
    /// its early returns — a `glDrawArrays` the state machine declines still
    /// costs the read and the log, and hiding that would be measuring the
    /// wrong thing.
    fn gl_draw(&mut self, mode: u32, first: u32, count: u32) {
        let t0 = std::time::Instant::now();
        // `fb_gen` counts the times the body actually rewrote `fb`; it is also
        // what the shell presents on. Comparing it across the call is the only
        // way to know a rebuild happened, because the body has five early
        // returns and an elision that all look the same from here.
        let gen = self.gl.fb_gen;
        self.gl_draw_body(mode, first, count);
        // On top of the finished picture, and only when the body actually
        // rebuilt it: an elided draw left `fb` alone, badges included, so
        // there is nothing to redo — and re-taking the "the HUD is in this
        // picture" flag on a frame that drew no picture would spend it on the
        // wrong one. See `padhud`.
        if self.gl.fb_gen != gen {
            self.pad_paint_badges();
            crate::hudedit::paint_overlay(self);
        }
        self.gl.draw_time += t0.elapsed();
    }

    fn gl_draw_body(&mut self, mode: u32, first: u32, count: u32) {
        self.gl.draws += 1;
        let Some(vp) = self.gl.vertex.clone() else {
            self.log.push(format!(
                "gl: DrawArrays(mode={mode:#x}, {first}, {count}) with no vertex pointer — skipped"
            ));
            return;
        };
        let Some(verts) = self.read_attrib(&vp, first, count) else {
            self.log.push(format!(
                "gl: DrawArrays vertex array unreadable (type={:#x} stride={} addr={:#x}) — skipped",
                vp.ty, vp.stride, vp.addr
            ));
            return;
        };
        let screen: Vec<(f32, f32)> = verts.iter().map(|&[x, y]| self.to_screen(x, y)).collect();
        let (mut dx0, mut dx1) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut dy0, mut dy1) = (f32::INFINITY, f32::NEG_INFINITY);
        for &(sx, sy) in &screen {
            dx0 = dx0.min(sx);
            dx1 = dx1.max(sx);
            dy0 = dy0.min(sy);
            dy1 = dy1.max(sy);
        }

        let x0 = (dx0.floor() as i32).clamp(0, self.gl.fb_w);
        let x1 = (dx1.ceil() as i32).clamp(0, self.gl.fb_w);
        let y0 = (dy0.floor() as i32).clamp(0, self.gl.fb_h);
        let y1 = (dy1.ceil() as i32).clamp(0, self.gl.fb_h);
        if x1 <= x0 || y1 <= y0 {
            return;
        }

        // Texture path: sample the bound texture across the quad's texcoord
        // bounding box. Anything else (untextured quad) fills with the color.
        let tex_rect = if self.gl.client.contains(&GL_TEXTURE_COORD_ARRAY) {
            self.gl.texcoord.clone().and_then(|tp| {
                let tcs = self.read_attrib(&tp, first, count)?;
                let (u0, u1) = tcs
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), t| (a.min(t[0]), b.max(t[0])));
                let (v0, v1) = tcs
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), t| (a.min(t[1]), b.max(t[1])));
                Some((u0, v0, u1, v1))
            })
        } else {
            None
        };

        let c = self.gl.color;
        let fill = ((c[0].clamp(0.0, 1.0) * 255.0) as u32) << 16
            | ((c[1].clamp(0.0, 1.0) * 255.0) as u32) << 8
            | (c[2].clamp(0.0, 1.0) * 255.0) as u32;

        let key = DrawKey {
            bound: self.gl.bound,
            tex_gen: self.gl.tex_gen,
            rect: (x0, x1, y0, y1),
            tex: tex_rect.map(|(u0, v0, u1, v1)| {
                [u0.to_bits(), v0.to_bits(), u1.to_bits(), v1.to_bits()]
            }),
            fill,
            fb: (self.gl.fb_w, self.gl.fb_h),
        };
        if self.gl.last_draw.as_ref() == Some(&key) {
            self.gl.draws_elided += 1;
            return;
        }
        self.gl.last_draw = Some(key);
        self.gl.fb_gen += 1;

        let GlState { textures, bound, fb, fb_w, .. } = &mut self.gl;
        let tex = tex_rect.and_then(|r| textures.get(bound).map(|t| (r, t)));

        // This loop is the single most expensive thing the host does per
        // frame, and none of it is the guest's: the destination is the whole
        // window (2220x1080 on a Galaxy S9) while the source is the 400x240
        // WIPI screen, so it is ~2.4 M samples every frame however fast the
        // interpreter runs. Written per-pixel it also carried two float
        // divides each, which measured as the bulk of a frame once the guest
        // blitters were ported.
        //
        // The column index depends only on `x` and the row only on `y`, so
        // both are computed once with the same operations in the same order,
        // and the inner loop is left with a table lookup and a store. This is
        // an arrangement change, not an arithmetic one; the frame A/B
        // (`ab_compare.py`) is what says so.
        //
        // # The `+ 0.5` is not a rounding preference, it is the fix for a
        // # dropped scanline
        //
        // These two lines used to sample the destination pixel's **left/top
        // edge** — `(x - x0) / dw` — and this quad is 1:1 (the guest issues
        // `glViewport(w, h)` and `glOrthof(0, w, 0, h)` against a texture it
        // sized to the canvas), so the map ought to be the identity. It was
        // not: `(x as f32 / w) * w` lands a hair *below* `x` for some `x`, and
        // the truncating cast then turns that into `x - 1`. On a 240-row
        // canvas the model says lines {63, 125, 126, 127} mis-sample, and
        // every frame dump this project has ever produced agrees — **rows 63
        // and 125 are byte-identical to the row above them, in every scene**,
        // which means the guest drew 240 rows and 238 of them reached the
        // screen. Magnified 4.5x on the phone that is two missing 4-pixel
        // bands.
        //
        // Sampling the pixel *centre* removes it: `(x + 0.5) / w * w` is
        // `x + 0.5`, which truncates to `x` for every `x`. Verified
        // exhaustively for every extent 2..=1024 by the test below.
        // **`f64` does not fix this** — it is a fencepost, not precision.
        let (dw, dh) = ((x1 - x0) as f32, (y1 - y0) as f32);
        match &tex {
            Some(((u0, v0, u1, v1), t)) if t.w > 0 && t.h > 0 => {
                let bpt = bytes_per_texel(t.format, t.ty);
                let cols: Vec<usize> = (x0..x1)
                    .map(|x| {
                        let fu = ((x - x0) as f32 + 0.5) / dw;
                        let tx = ((u0 + fu * (u1 - u0)) * t.w as f32) as i32;
                        tx.clamp(0, t.w - 1) as usize
                    })
                    .collect();
                for y in y0..y1 {
                    let fv = ((y - y0) as f32 + 0.5) / dh;
                    let ty = ((v0 + fv * (v1 - v0)) * t.h as f32) as i32;
                    let ty = ty.clamp(0, t.h - 1) as usize;
                    let trow = ty * t.w as usize;
                    let drow = (y * *fb_w + x0) as usize;
                    for (i, &tx) in cols.iter().enumerate() {
                        let off = (trow + tx) * bpt;
                        fb[drow + i] = match t.pixels.get(off..off + bpt) {
                            Some(p) => texel(p, t.format, t.ty),
                            None => fill,
                        };
                    }
                }
            }
            // Untextured quad: a flat fill, which is what the per-pixel
            // version degenerated to.
            _ => {
                for y in y0..y1 {
                    let drow = (y * *fb_w + x0) as usize;
                    fb[drow..drow + (x1 - x0) as usize].fill(fill);
                }
            }
        }
    }

    // ---- dispatch -----------------------------------------------------------

    pub(crate) fn dispatch_gl(&mut self, name: &str) -> Result<bool, String> {
        match name {
            "glGenTextures" => {
                let (n, ptr) = (self.arg(0), self.arg(1));
                for i in 0..n {
                    let name = self.gl.next_name;
                    self.gl.next_name += 1;
                    self.mem.write_u32(ptr + i * 4, name);
                }
                self.ret(0);
            }
            "glDeleteTextures" => {
                let (n, ptr) = (self.arg(0), self.arg(1));
                for i in 0..n {
                    let t = self.mem.read_u32(ptr + i * 4);
                    self.gl.textures.remove(&t);
                    self.gl.tex_gen += 1;
                    if self.gl.bound == t {
                        self.gl.bound = 0;
                    }
                }
                self.ret(0);
            }
            "glBindTexture" => {
                self.gl.bound = self.arg(1);
                self.ret(0);
            }
            "glTexImage2D" => {
                // (target, level, internal, w, h, border, format, type, pixels)
                let (w, h) = (self.arg(3) as i32, self.arg(4) as i32);
                let (format, ty, pix) = (self.arg(6), self.arg(7), self.arg(8));
                let bpt = bytes_per_texel(format, ty);
                let total = (w.max(0) as usize) * (h.max(0) as usize) * bpt;
                let pixels = if pix != 0 {
                    self.mem.slice(pix, total as u32).map(<[u8]>::to_vec).unwrap_or_else(|| vec![0; total])
                } else {
                    vec![0; total]
                };
                self.gl.textures.insert(self.gl.bound, Texture { w, h, format, ty, pixels });
                self.gl.tex_gen += 1;
                self.ret(0);
            }
            "glTexSubImage2D" => {
                // (target, level, xoff, yoff, w, h, format, type, pixels)
                let (xo, yo) = (self.arg(2) as i32, self.arg(3) as i32);
                let (w, h) = (self.arg(4) as i32, self.arg(5) as i32);
                let (format, ty, pix) = (self.arg(6), self.arg(7), self.arg(8));
                let bpt = bytes_per_texel(format, ty);
                let row = (w.max(0) as usize) * bpt;
                let src = self
                    .mem
                    .slice(pix, (row * h.max(0) as usize) as u32)
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default();
                let bound = self.gl.bound;
                if let Some(t) = self.gl.textures.get_mut(&bound) {
                    for r in 0..h.max(0) {
                        let sy = yo + r;
                        if sy < 0 || sy >= t.h {
                            continue;
                        }
                        let doff = ((sy * t.w + xo).max(0) as usize) * bpt;
                        let soff = r as usize * row;
                        let n = row.min(t.pixels.len().saturating_sub(doff));
                        if let (Some(d), Some(s)) =
                            (t.pixels.get_mut(doff..doff + n), src.get(soff..soff + n))
                        {
                            d.copy_from_slice(s);
                        }
                    }
                }
                self.gl.tex_gen += 1;
                self.ret(0);
            }
            "glDrawArrays" => {
                let (mode, first, count) = (self.arg(0), self.arg(1), self.arg(2));
                self.gl_draw(mode, first, count);
                self.ret(0);
            }
            "glVertexPointer" | "glTexCoordPointer" => {
                let p = Pointer {
                    size: self.arg(0),
                    ty: self.arg(1),
                    stride: self.arg(2),
                    addr: self.arg(3),
                };
                if name == "glVertexPointer" {
                    self.gl.vertex = Some(p);
                } else {
                    self.gl.texcoord = Some(p);
                }
                self.ret(0);
            }
            "glEnableClientState" | "glDisableClientState" => {
                let cap = self.arg(0);
                if name.starts_with("glEnable") {
                    self.gl.client.insert(cap);
                } else {
                    self.gl.client.remove(&cap);
                }
                let _ = GL_VERTEX_ARRAY; // documented for readers; only TEXTURE_COORD_ARRAY gates behaviour
                self.ret(0);
            }
            "glEnable" | "glDisable" => {
                let cap = self.arg(0);
                if name == "glEnable" {
                    self.gl.caps.insert(cap);
                } else {
                    self.gl.caps.remove(&cap);
                }
                self.ret(0);
            }
            "glMatrixMode" => {
                self.gl.matrix_mode = self.arg(0);
                self.ret(0);
            }
            "glLoadIdentity" => {
                if self.gl.matrix_mode == GL_PROJECTION {
                    self.gl.ortho = None;
                }
                self.ret(0);
            }
            "glOrthof" => {
                // 6 f32 arguments: r0-r3 then two stack slots.
                let o = [
                    self.argf(0),
                    self.argf(1),
                    self.argf(2),
                    self.argf(3),
                    self.argf(4),
                    self.argf(5),
                ];
                if self.gl.matrix_mode == GL_PROJECTION {
                    self.gl.ortho = Some(o);
                }
                self.ret(0);
            }
            "glViewport" => {
                self.gl.viewport = (
                    self.arg(0) as i32,
                    self.arg(1) as i32,
                    self.arg(2) as i32,
                    self.arg(3) as i32,
                );
                self.ret(0);
            }
            "glColor4f" => {
                self.gl.color = [self.argf(0), self.argf(1), self.argf(2), self.argf(3)];
                self.ret(0);
            }
            "glTexParameterf" | "glCullFace" => {
                // Filtering and culling have no observable effect on a
                // nearest-neighbour full-quad blit; recorded by the trace,
                // nothing to store.
                self.ret(0);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    /// The sample-index arithmetic of [`Vm::gl_draw_body`], isolated: one
    /// destination index in `0..extent` against a texture of the same extent,
    /// which is the 1:1 quad this guest always issues.
    fn sample(d: i32, extent: i32) -> i32 {
        let span = extent as f32;
        let f = (d as f32 + 0.5) / span;
        ((0.0 + f * 1.0) * span) as i32
    }

    /// **The bug this replaced dropped two scanlines of every frame.** With
    /// the destination pixel's edge instead of its centre, a 240-row canvas
    /// mis-sampled rows 63, 125, 126 and 127, so rows 63 and 127 never reached
    /// the screen at all — visible in every frame dump in the repository as
    /// row 63 and row 125 being byte-identical to the row above.
    ///
    /// The property that has to hold is stronger than "no duplicates": on a
    /// 1:1 map every source line must be used exactly once, and it has to hold
    /// for every canvas anyone might configure, not just 240.
    #[test]
    fn a_one_to_one_quad_uses_every_source_line_exactly_once() {
        for extent in 2..=1024 {
            let mut seen = vec![false; extent as usize];
            for d in 0..extent {
                let s = sample(d, extent).clamp(0, extent - 1);
                assert_eq!(s, d, "extent {extent}: destination {d} sampled source {s}");
                assert!(!seen[s as usize], "extent {extent}: source {s} used twice");
                seen[s as usize] = true;
            }
        }
    }

    /// And the old expression really did fail it — kept so the test cannot
    /// quietly stop testing anything if someone "simplifies" `sample`.
    #[test]
    fn the_edge_sampling_it_replaced_really_was_broken() {
        let edge = |d: i32, extent: i32| -> i32 {
            let span = extent as f32;
            (((d as f32 / span) * 1.0) * span) as i32
        };
        let bad: Vec<i32> = (0..240).filter(|&d| edge(d, 240) != d).collect();
        assert_eq!(bad, vec![63, 125, 126, 127], "the 240-row canvas this game ships with");
    }
}
