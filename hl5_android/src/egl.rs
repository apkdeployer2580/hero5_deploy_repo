//! The GPU presenter: EGL + GLES2, replacing the CPU scale-and-copy.
//!
//! # Why this exists
//!
//! §33.0 measured present at **7.06 ms per repaint** on the S25 while the
//! whole repaint had 36 ms of guest design budget and was spending 38.8 —
//! the loop is single-threaded, so present is the difference between the
//! gameplay scene sitting at 25.8 repaints/s and reaching the guest's 27.8
//! ceiling. What those milliseconds bought was a CPU writing 2,340×1,080
//! words through softbuffer's staging buffer. The GPU does the same scale as
//! a texture fetch it was built for: upload one canvas-sized (or, with
//! Scale2x in front, doubled) texture per *changed* frame and let a
//! full-window quad sample it.
//!
//! # What it must preserve (§32.10, §33)
//!
//! * **Nearest sampling, fragment centers.** The GPU fetches
//!   `floor((x + 0.5) * src / dst)` — texel centers, the §32.10 convention.
//!   The CPU window loop it replaces samples *edges* (`floor(x * src /
//!   dst)`); the two floors agree on every pixel exactly when the ratio is
//!   an integer, and the shipped canvas is integer by §33's construction
//!   (585×270 → ×2 → ×2 on the S25), so the shipped picture is identical.
//!   At a non-integer ratio — split screen, a fold, any surface the canvas
//!   no longer divides — soft and egl can differ by a one-texel phase;
//!   that is a known difference, not a regression. Bilinear is not an
//!   upgrade either way: it invents colour on 24% of text pixels on this
//!   art (§32.10).
//! * **Scale2x stays on the CPU**, in front, unchanged (`App::scale2x`,
//!   measured +0.5 ms/frame). This module only replaces the *last* hop.
//! * **Present-skip stays in `App::present`**: this is only called for a
//!   frame whose bytes actually changed.
//!
//! # Contract with the caller
//!
//! Built in `configure_surface` while the window is alive, torn down whole
//! in `suspended` — the `EGLSurface` wraps the `ANativeWindow` that dies
//! with the Activity's surface, and a from-scratch rebuild once per
//! foreground transition is the lifetime that cannot dangle (whether the
//! rebuild is felt is on §34.4's device list). Every failure is an `Err`, never a
//! panic: the caller logs it and falls back to the softbuffer path, which
//! stays fully wired — a slower game rather than no game.
//!
//! No new crates: the six dozen symbols needed are declared by hand against
//! the system `libEGL.so` / `libGLESv2.so`, which every Android since API 8
//! ships (the NDK provides the link stubs). The desktop build of this crate
//! compiles a stub that always declines, so the host `cargo check` still
//! covers the call sites.

use std::time::Duration;
#[cfg(not(target_os = "android"))]
use winit::window::Window;

/// (texture upload, draw + swap) — split so the periodic line's `blit` and
/// `swap` keep meaning "our copy" and "handing it to the compositor".
pub type PresentTimes = (Duration, Duration);

#[cfg(target_os = "android")]
#[allow(non_snake_case, non_camel_case_types, dead_code, clippy::upper_case_acronyms)]
mod ffi {
    use std::ffi::{c_char, c_void};

    pub type EGLBoolean = u32;
    pub type EGLint = i32;
    pub type EGLDisplay = *mut c_void;
    pub type EGLConfig = *mut c_void;
    pub type EGLSurface = *mut c_void;
    pub type EGLContext = *mut c_void;
    pub type EGLNativeDisplayType = *mut c_void;
    pub type EGLNativeWindowType = *mut c_void;

    pub const EGL_DEFAULT_DISPLAY: EGLNativeDisplayType = std::ptr::null_mut();
    pub const EGL_NO_DISPLAY: EGLDisplay = std::ptr::null_mut();
    pub const EGL_NO_SURFACE: EGLSurface = std::ptr::null_mut();
    pub const EGL_NO_CONTEXT: EGLContext = std::ptr::null_mut();
    pub const EGL_FALSE: EGLBoolean = 0;
    pub const EGL_NONE: EGLint = 0x3038;
    pub const EGL_BLUE_SIZE: EGLint = 0x3022;
    pub const EGL_GREEN_SIZE: EGLint = 0x3023;
    pub const EGL_RED_SIZE: EGLint = 0x3024;
    pub const EGL_SURFACE_TYPE: EGLint = 0x3033;
    pub const EGL_WINDOW_BIT: EGLint = 0x0004;
    pub const EGL_RENDERABLE_TYPE: EGLint = 0x3040;
    pub const EGL_OPENGL_ES2_BIT: EGLint = 0x0004;
    pub const EGL_CONTEXT_CLIENT_VERSION: EGLint = 0x3098;
    pub const EGL_WIDTH: EGLint = 0x3057;
    pub const EGL_HEIGHT: EGLint = 0x3056;

    pub type GLenum = u32;
    pub type GLuint = u32;
    pub type GLint = i32;
    pub type GLsizei = i32;
    pub type GLbitfield = u32;
    pub type GLfloat = f32;
    pub type GLboolean = u8;
    pub type GLchar = c_char;

    pub const GL_TEXTURE_2D: GLenum = 0x0DE1;
    pub const GL_RGBA: GLenum = 0x1908;
    pub const GL_UNSIGNED_BYTE: GLenum = 0x1401;
    pub const GL_TEXTURE_MAG_FILTER: GLenum = 0x2800;
    pub const GL_TEXTURE_MIN_FILTER: GLenum = 0x2801;
    pub const GL_NEAREST: GLint = 0x2600;
    pub const GL_TEXTURE_WRAP_S: GLenum = 0x2802;
    pub const GL_TEXTURE_WRAP_T: GLenum = 0x2803;
    pub const GL_CLAMP_TO_EDGE: GLint = 0x812F;
    pub const GL_COLOR_BUFFER_BIT: GLbitfield = 0x4000;
    pub const GL_TRIANGLE_STRIP: GLenum = 0x0005;
    pub const GL_FLOAT: GLenum = 0x1406;
    pub const GL_FRAGMENT_SHADER: GLenum = 0x8B30;
    pub const GL_VERTEX_SHADER: GLenum = 0x8B31;
    pub const GL_COMPILE_STATUS: GLenum = 0x8B81;
    pub const GL_LINK_STATUS: GLenum = 0x8B82;
    pub const GL_DITHER: GLenum = 0x0BD0;
    pub const GL_RENDERER: GLenum = 0x1F01;
    pub const GL_NO_ERROR: GLenum = 0;

    #[link(name = "EGL")]
    extern "C" {
        pub fn eglGetDisplay(display_id: EGLNativeDisplayType) -> EGLDisplay;
        pub fn eglInitialize(dpy: EGLDisplay, major: *mut EGLint, minor: *mut EGLint) -> EGLBoolean;
        pub fn eglChooseConfig(
            dpy: EGLDisplay,
            attrib_list: *const EGLint,
            configs: *mut EGLConfig,
            config_size: EGLint,
            num_config: *mut EGLint,
        ) -> EGLBoolean;
        pub fn eglCreateWindowSurface(
            dpy: EGLDisplay,
            config: EGLConfig,
            win: EGLNativeWindowType,
            attrib_list: *const EGLint,
        ) -> EGLSurface;
        pub fn eglCreateContext(
            dpy: EGLDisplay,
            config: EGLConfig,
            share_context: EGLContext,
            attrib_list: *const EGLint,
        ) -> EGLContext;
        pub fn eglMakeCurrent(
            dpy: EGLDisplay,
            draw: EGLSurface,
            read: EGLSurface,
            ctx: EGLContext,
        ) -> EGLBoolean;
        pub fn eglSwapBuffers(dpy: EGLDisplay, surface: EGLSurface) -> EGLBoolean;
        pub fn eglSwapInterval(dpy: EGLDisplay, interval: EGLint) -> EGLBoolean;
        pub fn eglQuerySurface(
            dpy: EGLDisplay,
            surface: EGLSurface,
            attribute: EGLint,
            value: *mut EGLint,
        ) -> EGLBoolean;
        pub fn eglDestroySurface(dpy: EGLDisplay, surface: EGLSurface) -> EGLBoolean;
        pub fn eglDestroyContext(dpy: EGLDisplay, ctx: EGLContext) -> EGLBoolean;
        pub fn eglTerminate(dpy: EGLDisplay) -> EGLBoolean;
        pub fn eglReleaseThread() -> EGLBoolean;
        pub fn eglGetError() -> EGLint;
    }

    #[link(name = "GLESv2")]
    extern "C" {
        pub fn glCreateShader(kind: GLenum) -> GLuint;
        pub fn glShaderSource(
            shader: GLuint,
            count: GLsizei,
            string: *const *const GLchar,
            length: *const GLint,
        );
        pub fn glCompileShader(shader: GLuint);
        pub fn glGetShaderiv(shader: GLuint, pname: GLenum, params: *mut GLint);
        pub fn glGetShaderInfoLog(
            shader: GLuint,
            buf_size: GLsizei,
            length: *mut GLsizei,
            info_log: *mut GLchar,
        );
        pub fn glDeleteShader(shader: GLuint);
        pub fn glCreateProgram() -> GLuint;
        pub fn glAttachShader(program: GLuint, shader: GLuint);
        pub fn glLinkProgram(program: GLuint);
        pub fn glGetProgramiv(program: GLuint, pname: GLenum, params: *mut GLint);
        pub fn glGetProgramInfoLog(
            program: GLuint,
            buf_size: GLsizei,
            length: *mut GLsizei,
            info_log: *mut GLchar,
        );
        pub fn glUseProgram(program: GLuint);
        pub fn glGetAttribLocation(program: GLuint, name: *const GLchar) -> GLint;
        pub fn glGetUniformLocation(program: GLuint, name: *const GLchar) -> GLint;
        pub fn glUniform1i(location: GLint, v0: GLint);
        pub fn glVertexAttribPointer(
            index: GLuint,
            size: GLint,
            kind: GLenum,
            normalized: GLboolean,
            stride: GLsizei,
            pointer: *const std::ffi::c_void,
        );
        pub fn glEnableVertexAttribArray(index: GLuint);
        pub fn glGenTextures(n: GLsizei, textures: *mut GLuint);
        pub fn glBindTexture(target: GLenum, texture: GLuint);
        pub fn glTexParameteri(target: GLenum, pname: GLenum, param: GLint);
        pub fn glTexImage2D(
            target: GLenum,
            level: GLint,
            internal_format: GLint,
            width: GLsizei,
            height: GLsizei,
            border: GLint,
            format: GLenum,
            kind: GLenum,
            pixels: *const std::ffi::c_void,
        );
        pub fn glTexSubImage2D(
            target: GLenum,
            level: GLint,
            xoffset: GLint,
            yoffset: GLint,
            width: GLsizei,
            height: GLsizei,
            format: GLenum,
            kind: GLenum,
            pixels: *const std::ffi::c_void,
        );
        pub fn glViewport(x: GLint, y: GLint, width: GLsizei, height: GLsizei);
        pub fn glClear(mask: GLbitfield);
        pub fn glClearColor(red: GLfloat, green: GLfloat, blue: GLfloat, alpha: GLfloat);
        pub fn glDisable(cap: GLenum);
        pub fn glDrawArrays(mode: GLenum, first: GLint, count: GLsizei);
        pub fn glGetError() -> GLenum;
        pub fn glGetString(name: GLenum) -> *const u8;
    }
}

#[cfg(target_os = "android")]
pub use android::EglPresenter;

#[cfg(target_os = "android")]
mod android {
    use super::ffi::*;
    use super::PresentTimes;
    use std::ffi::{c_void, CStr};
    use std::time::Instant;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    /// The framebuffer is `0x00RRGGBB` words. Read as `GL_RGBA` bytes on
    /// this little-endian target, a texel arrives as (r=B, g=G, b=R, a=0);
    /// the swizzle puts the channels back and the constant `1.0` alpha is
    /// load-bearing — the guest's alpha byte is 0, and an EGL config with an
    /// alpha channel would otherwise hand the compositor a fully transparent
    /// window. `GL_BGRA` would avoid the swizzle but is an extension in
    /// GLES2; a swizzle in the shader costs nothing and works everywhere.
    ///
    /// Precision: `v_uv` steps in units of 1/2340 on the S25, below fp16
    /// resolution, and a mediump interpolator can land a nearest fetch on
    /// the wrong texel — a column duplicated here and dropped there, the
    /// same class of bug as the §32.10 fencepost. `highp` where the GPU has
    /// it (everything this app can install on), the standard fallback where
    /// not.
    const VERT_SRC: &CStr = c"
        attribute vec2 a_pos;
        varying vec2 v_uv;
        void main() {
            v_uv = vec2(a_pos.x * 0.5 + 0.5, 0.5 - a_pos.y * 0.5);
            gl_Position = vec4(a_pos, 0.0, 1.0);
        }";
    const FRAG_SRC: &CStr = c"
        #ifdef GL_FRAGMENT_PRECISION_HIGH
        precision highp float;
        #else
        precision mediump float;
        #endif
        varying vec2 v_uv;
        uniform sampler2D u_tex;
        void main() {
            vec4 t = texture2D(u_tex, v_uv);
            gl_FragColor = vec4(t.b, t.g, t.r, 1.0);
        }";

    /// Two triangles covering the whole viewport. `static` because the
    /// vertex pointer is client memory read at every draw — GLES2 has no
    /// mandatory buffer objects and four vertices do not earn one.
    static QUAD: [GLfloat; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];

    pub struct EglPresenter {
        display: EGLDisplay,
        surface: EGLSurface,
        context: EGLContext,
        tex: GLuint,
        /// Texture allocation as of the last upload. A size change (Scale2x
        /// toggled, a fold) reallocates with `glTexImage2D`; the steady
        /// state is `glTexSubImage2D` into the same storage.
        tex_w: GLsizei,
        tex_h: GLsizei,
        renderer: String,
    }

    impl EglPresenter {
        /// Stand the whole pipeline up against the window's `ANativeWindow`,
        /// or say exactly which step declined. Called on the winit thread and
        /// the context stays current on it — the VM, the presenter and the
        /// event loop share that thread by design (crate doc).
        pub fn new(window: &Window) -> Result<Self, String> {
            let handle = window
                .window_handle()
                .map_err(|e| format!("no window handle: {e}"))?
                .as_raw();
            let native = match handle {
                RawWindowHandle::AndroidNdk(h) => h.a_native_window.as_ptr(),
                other => return Err(format!("not an Android native window: {other:?}")),
            };
            // Everything below is raw EGL/GL. Partial failures tear down what
            // was built so a declined presenter leaks nothing — `fail`
            // mirrors `Drop` for a half-built state.
            unsafe { Self::build(native) }
        }

        unsafe fn build(native: *mut c_void) -> Result<Self, String> {
            unsafe fn fail(
                dpy: EGLDisplay,
                surf: EGLSurface,
                ctx: EGLContext,
                msg: String,
            ) -> Result<EglPresenter, String> {
                // Captured before the teardown: EGL's error state is
                // per-thread and reflects the most recent call, so the
                // successful cleanup below would overwrite the interesting
                // value with EGL_SUCCESS.
                let err = eglGetError();
                if ctx != EGL_NO_CONTEXT {
                    eglMakeCurrent(dpy, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
                    eglDestroyContext(dpy, ctx);
                }
                if surf != EGL_NO_SURFACE {
                    eglDestroySurface(dpy, surf);
                }
                if dpy != EGL_NO_DISPLAY {
                    eglTerminate(dpy);
                    eglReleaseThread();
                }
                Err(format!("{msg} (egl error {err:#x})"))
            }

            let dpy = eglGetDisplay(EGL_DEFAULT_DISPLAY);
            if dpy == EGL_NO_DISPLAY {
                return fail(dpy, EGL_NO_SURFACE, EGL_NO_CONTEXT, "eglGetDisplay".into());
            }
            let (mut major, mut minor): (EGLint, EGLint) = (0, 0);
            if eglInitialize(dpy, &mut major, &mut minor) == EGL_FALSE {
                return fail(dpy, EGL_NO_SURFACE, EGL_NO_CONTEXT, "eglInitialize".into());
            }
            // 888 colour on a window surface, ES2-renderable. No alpha asked
            // for; if the chosen config has one anyway, the shader's 1.0
            // covers it.
            let attribs: [EGLint; 11] = [
                EGL_RENDERABLE_TYPE,
                EGL_OPENGL_ES2_BIT,
                EGL_SURFACE_TYPE,
                EGL_WINDOW_BIT,
                EGL_RED_SIZE,
                8,
                EGL_GREEN_SIZE,
                8,
                EGL_BLUE_SIZE,
                8,
                EGL_NONE,
            ];
            let mut config: EGLConfig = std::ptr::null_mut();
            let mut count: EGLint = 0;
            if eglChooseConfig(dpy, attribs.as_ptr(), &mut config, 1, &mut count) == EGL_FALSE
                || count < 1
            {
                return fail(dpy, EGL_NO_SURFACE, EGL_NO_CONTEXT, "eglChooseConfig".into());
            }
            let none = [EGL_NONE];
            let surf = eglCreateWindowSurface(dpy, config, native, none.as_ptr());
            if surf == EGL_NO_SURFACE {
                return fail(dpy, surf, EGL_NO_CONTEXT, "eglCreateWindowSurface".into());
            }
            let ctx_attribs = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
            let ctx = eglCreateContext(dpy, config, EGL_NO_CONTEXT, ctx_attribs.as_ptr());
            if ctx == EGL_NO_CONTEXT {
                return fail(dpy, surf, ctx, "eglCreateContext".into());
            }
            if eglMakeCurrent(dpy, surf, surf, ctx) == EGL_FALSE {
                return fail(dpy, surf, ctx, "eglMakeCurrent".into());
            }
            // Do not throttle the swap to vblank. Pacing already lives on the
            // shell's FRAME grid, and a blocking swap would put the vsync
            // wait back on the one thread the guest runs on — the exact cost
            // this module removes. No tearing risk: SurfaceFlinger always
            // composites whole buffers, and at <28 presents/s against a
            // 120 Hz display the BufferQueue never fills (§31.3).
            eglSwapInterval(dpy, 0);

            let program = match Self::link_program() {
                Ok(p) => p,
                Err(e) => return fail(dpy, surf, ctx, e),
            };
            glUseProgram(program);
            let a_pos = glGetAttribLocation(program, c"a_pos".as_ptr());
            let u_tex = glGetUniformLocation(program, c"u_tex".as_ptr());
            if a_pos < 0 || u_tex < 0 {
                return fail(dpy, surf, ctx, "attrib/uniform lookup failed".into());
            }
            // All of this is context state and nothing else in the process
            // touches the context, so it is set once here rather than per
            // frame.
            glUniform1i(u_tex, 0);
            glVertexAttribPointer(a_pos as GLuint, 2, GL_FLOAT, 0, 0, QUAD.as_ptr().cast());
            glEnableVertexAttribArray(a_pos as GLuint);
            // Dithering defaults ON in GLES and would perturb exact colours;
            // this art wants the words it authored (§32.10 measured filters
            // by "colours invented", and the answer that won invents none).
            glDisable(GL_DITHER);
            glClearColor(0.0, 0.0, 0.0, 1.0);

            let mut tex: GLuint = 0;
            glGenTextures(1, &mut tex);
            glBindTexture(GL_TEXTURE_2D, tex);
            // NEAREST is the picture contract; CLAMP_TO_EDGE + no mipmaps is
            // also what makes a non-power-of-two texture (585×270, 1170×540)
            // legal in core GLES2.
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);

            let err = glGetError();
            if err != GL_NO_ERROR {
                return fail(dpy, surf, ctx, format!("GL setup error {err:#x}"));
            }
            let renderer = {
                let p = glGetString(GL_RENDERER);
                if p.is_null() {
                    "unknown renderer".to_string()
                } else {
                    CStr::from_ptr(p.cast()).to_string_lossy().into_owned()
                }
            };
            Ok(EglPresenter {
                display: dpy,
                surface: surf,
                context: ctx,
                tex,
                tex_w: 0,
                tex_h: 0,
                renderer,
            })
        }

        unsafe fn compile(kind: GLenum, src: &CStr) -> Result<GLuint, String> {
            let shader = glCreateShader(kind);
            if shader == 0 {
                return Err(format!("glCreateShader({kind:#x}) returned 0"));
            }
            let p = src.as_ptr();
            glShaderSource(shader, 1, &p, std::ptr::null());
            glCompileShader(shader);
            let mut ok: GLint = 0;
            glGetShaderiv(shader, GL_COMPILE_STATUS, &mut ok);
            if ok == 0 {
                let mut buf = [0u8; 512];
                let mut len: GLsizei = 0;
                glGetShaderInfoLog(shader, buf.len() as GLsizei, &mut len, buf.as_mut_ptr().cast());
                glDeleteShader(shader);
                let len = (len.max(0) as usize).min(buf.len());
                return Err(format!(
                    "shader {kind:#x} compile: {}",
                    String::from_utf8_lossy(&buf[..len])
                ));
            }
            Ok(shader)
        }

        unsafe fn link_program() -> Result<GLuint, String> {
            let vs = Self::compile(GL_VERTEX_SHADER, VERT_SRC)?;
            let fs = match Self::compile(GL_FRAGMENT_SHADER, FRAG_SRC) {
                Ok(s) => s,
                Err(e) => {
                    glDeleteShader(vs);
                    return Err(e);
                }
            };
            let program = glCreateProgram();
            if program == 0 {
                glDeleteShader(vs);
                glDeleteShader(fs);
                return Err("glCreateProgram returned 0".into());
            }
            glAttachShader(program, vs);
            glAttachShader(program, fs);
            glLinkProgram(program);
            // Flagged for deletion now, freed with the program; freed with
            // the context either way.
            glDeleteShader(vs);
            glDeleteShader(fs);
            let mut ok: GLint = 0;
            glGetProgramiv(program, GL_LINK_STATUS, &mut ok);
            if ok == 0 {
                let mut buf = [0u8; 512];
                let mut len: GLsizei = 0;
                glGetProgramInfoLog(program, buf.len() as GLsizei, &mut len, buf.as_mut_ptr().cast());
                let len = (len.max(0) as usize).min(buf.len());
                return Err(format!("program link: {}", String::from_utf8_lossy(&buf[..len])));
            }
            Ok(program)
        }

        /// One changed frame to the screen: upload, draw, swap.
        ///
        /// `px` is `0x00RRGGBB` words, `w * h` of them — either the canvas
        /// or its Scale2x double; this function does not care which, it
        /// reallocates on any size change. An `Err` from the swap is the
        /// caller's cue to retire this presenter and fall back.
        ///
        /// `dst` is the horizontal fraction of the window the picture may use,
        /// as `(numerator, denominator)` — the 해상도 setting
        /// (`hl5_core::screen`). `(n, n)` is the shipped full-width case and
        /// leaves the viewport exactly where it has always been; anything
        /// narrower is centred, and the `glClear` below is what paints the
        /// bands on either side.
        pub fn present(
            &mut self,
            px: &[u32],
            w: i32,
            h: i32,
            dst: (i32, i32),
        ) -> Result<PresentTimes, String> {
            let (w, h) = (w.max(1), h.max(1));
            if px.len() < (w as usize) * (h as usize) {
                return Err(format!("frame {}px short of {w}x{h}", px.len()));
            }
            unsafe {
                let t0 = Instant::now();
                glBindTexture(GL_TEXTURE_2D, self.tex);
                if (w, h) != (self.tex_w, self.tex_h) {
                    glTexImage2D(
                        GL_TEXTURE_2D,
                        0,
                        GL_RGBA as GLint,
                        w,
                        h,
                        0,
                        GL_RGBA,
                        GL_UNSIGNED_BYTE,
                        px.as_ptr().cast(),
                    );
                    self.tex_w = w;
                    self.tex_h = h;
                } else {
                    glTexSubImage2D(
                        GL_TEXTURE_2D,
                        0,
                        0,
                        0,
                        w,
                        h,
                        GL_RGBA,
                        GL_UNSIGNED_BYTE,
                        px.as_ptr().cast(),
                    );
                }
                let upload = t0.elapsed();

                let t1 = Instant::now();
                // The surface's size, not the winit window's: during a
                // rotation the two disagree for a few frames, and the
                // authority on what eglSwapBuffers will show is EGL.
                let (mut sw, mut sh): (EGLint, EGLint) = (0, 0);
                eglQuerySurface(self.display, self.surface, EGL_WIDTH, &mut sw);
                eglQuerySurface(self.display, self.surface, EGL_HEIGHT, &mut sh);
                let (sw, sh) = (sw.max(1), sh.max(1));
                // Integer, and from the same ratio the canvas was narrowed
                // by, so the picture cannot drift half a device pixel away
                // from what the shell tells touches to expect.
                let vw = if dst.1 > 0 {
                    ((sw as i64 * dst.0 as i64) / dst.1 as i64).clamp(1, sw as i64) as GLsizei
                } else {
                    sw
                };
                glViewport((sw - vw) / 2, 0, vw, sh);
                // A clear before a full-viewport opaque quad looks redundant;
                // on the tiled GPUs in phones it is the hint that lets the
                // driver skip reloading the previous buffer's contents.
                glClear(GL_COLOR_BUFFER_BIT);
                glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
                if eglSwapBuffers(self.display, self.surface) == EGL_FALSE {
                    return Err(format!("eglSwapBuffers (egl error {:#x})", eglGetError()));
                }
                Ok((upload, t1.elapsed()))
            }
        }

        /// One line for the boot log, so a device report says which GPU and
        /// which path produced the numbers next to it.
        pub fn describe(&self) -> String {
            format!("EGL/GLES2 on {}", self.renderer)
        }
    }

    impl Drop for EglPresenter {
        fn drop(&mut self) {
            // Full teardown, including the display connection: nothing else
            // in the process speaks EGL, and `resumed` rebuilds from scratch.
            unsafe {
                eglMakeCurrent(self.display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
                eglDestroySurface(self.display, self.surface);
                eglDestroyContext(self.display, self.context);
                eglTerminate(self.display);
                eglReleaseThread();
            }
        }
    }
}

/// The desktop stub: always declines, so `App::present`'s fallback keeps the
/// softbuffer path — the one a desktop host actually uses — and the call
/// sites type-check without a single `cfg` outside this module.
#[cfg(not(target_os = "android"))]
pub struct EglPresenter;

#[cfg(not(target_os = "android"))]
impl EglPresenter {
    pub fn new(_window: &Window) -> Result<Self, String> {
        Err("EGL presenter is Android-only".into())
    }

    pub fn present(
        &mut self,
        _px: &[u32],
        _w: i32,
        _h: i32,
        _dst: (i32, i32),
    ) -> Result<PresentTimes, String> {
        Err("EGL presenter is Android-only".into())
    }

    pub fn describe(&self) -> String {
        String::new()
    }
}
