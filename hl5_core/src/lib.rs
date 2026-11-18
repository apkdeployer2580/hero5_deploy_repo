//! heroesLore5 native runtime.
//!
//! The shipped game is `lib/armeabi/libHeroesLore5.so`: 1.17 MB of ARMv5TE code
//! (3,394 named functions) that EA Mobile Korea produced by compiling the
//! original WIPI clet against their "midas" porting layer. There is no source,
//! and no tool turns ARM32 machine code into ARM64 machine code, so a literal
//! 32-to-64-bit rebuild of that library does not exist.
//!
//! What does exist is a very small seam. The library imports exactly 77 symbols
//! — libc, three libm functions, nineteen GLES 1.x calls and four C++ ABI
//! entries — exposes eighteen JNI entry points, and reaches back into Java
//! through nineteen static methods on one class. Everything else, including the
//! whole WIPI `MC_*` API, is inside the binary. So this crate runs the original
//! code on an ARMv5TE interpreter inside a pure AArch64 process and implements
//! that seam natively. The game logic is the shipped binary, bit for bit; the
//! host around it is 64-bit.
//!
//! ```text
//!   Android arm64-v8a app  (hl5_android)      headless probe  (hl5_probe)
//!                     \                      /
//!                      +---- hl5_core ------+
//!                      | Vm: cpu, memory,   |
//!                      | HLE libc/GL/JNI    |
//!                      +---------+----------+
//!                                |  ARMv5TE
//!                       libHeroesLore5.so (guest, unmodified)
//! ```
//!
//! See `ANALYSIS.md` for why this shape, and `CLAUDE.md` for the facts that
//! stay true between sessions.

pub mod audio;
pub mod avatar;
pub mod bossdrop;
pub mod cashitems;
pub mod cletentry;
pub mod costume;
mod costume_sets;
pub mod des;
pub mod elf;
pub mod exp;
pub mod files;
pub mod fmt;
pub mod gamepad;
pub mod gl;
pub mod heap;
pub mod hle;
pub mod input;
pub mod jni;
pub mod hud;
pub mod hudedit;
pub mod hudlayout;
pub mod igoptions;
pub mod legendaries;
pub mod localserver;
pub mod lostdrops;
pub mod mem;
pub mod memwatch;
pub mod npcbody;
pub mod options;
pub mod padmap;
pub mod padhud;
/// The internal file-storage root the guest is told about in
/// `nativeInitFileStorage`, and therefore the prefix
/// `MIDASKernelManager::getFullFilePath` (0x151E40) puts in front of **every**
/// `/c/...` path before `stat`/`open` ever see it: with mode 1 it is
/// `strcat(root)` + `'/'` + `strcat(path)` (0x151E70..0x151EA0).
///
/// So a host file meant to shadow an archive resource lives at
/// `<storage>/data/data/co.kr.eamobile.HeroesLore5/files/c/...`, not at
/// `<storage>/c/...`. [`rates`] depends on this and so does every save the
/// guest writes.
pub const INTERNAL_STORAGE_ROOT: &str = "/data/data/co.kr.eamobile.HeroesLore5/files";

pub mod patch;
pub mod portrait;
pub mod rates;
pub mod savebundle;
pub mod savedump;
pub mod savexfer;
pub mod screen;
pub use screen::ScreenMode;
pub mod shop;
pub mod skilltime;
pub mod spirit_avatar;
pub mod tenlists;
pub mod tenpages;
pub mod warehouse;

use arm32_cpu::{reg, Cpu, Mode};
use std::path::PathBuf;
use std::sync::Arc;

use crate::elf::Elf;
use crate::heap::Heap;
use crate::mem::{GuestMem, HEAP_BASE, HEAP_SIZE, IMAGE_BASE, RETURN_MAGIC, STACK_BASE, STACK_SIZE};

/// The WIPI logical screen this title was authored for. From
/// `co.kr.eamobile.CletEntry.adjustAutoScreen`, whose constants are 400 and 240
/// — the same pair the sibling chronosSword uses.
///
/// It is an *argument* to `nativeInitDisplay`, not a constant in the binary, so
/// other values are a legal experiment once the display path is up.
pub const WIPI_W: i32 = 400;
pub const WIPI_H: i32 = 240;

/// The logical canvas to give a display of this size: the display's aspect at
/// the game's design height.
///
/// A 400x240 picture on a 19.5:9 phone has three options, and only one of them
/// is good. Letterboxing it (what `adjustAutoScreen` does, and what this shell
/// shipped until now) wastes a quarter of the screen on black bars. Stretching
/// it to fill — scaling the axes independently — makes every sprite 30% too
/// wide and, because the factors are not integers, duplicates some columns six
/// times and others five, which reads as a broken picture rather than a wide
/// one. The third option is this: keep the scale *uniform* and give the guest a
/// canvas that is already the right shape, so the player simply sees more of
/// the scene to the left and right.
///
/// That works because 400x240 is an **argument** to `nativeInitDisplay`, not a
/// constant inside the game. Measured on this binary at 440, 480 and 520: the
/// menus, dialogs, class select and cash-shop panel re-centre, the minimap and
/// the skill/D-pad column stay pinned to the right edge, the HP bars stay
/// pinned to the left, and the guest draws more dungeon rather than a stretched
/// one. (The one exception is the in-game map/shop/save bar, which sits at a
/// fixed logical x and so drifts left of centre — see ANALYSIS.md §27.)
///
/// `long/short` rather than `w/h` because the game is landscape and
/// [`Vm::fit_screen`] already normalises the same way.
///
/// The floor is [`WIPI_W`], never lower: a canvas narrower than the authored
/// 400 would *crop* the UI instead of widening it, which is what a portrait
/// surface — or a first-frame size that has not settled yet — would otherwise
/// ask for. The ceiling is the guest's own: `Texture2D::getWidth2n`
/// (0x152af0) walks 2^0..2^10 and returns 0 past that, and `createTexture`
/// (0x15331c) bails on a zero width.
///
/// Flooring, not rounding, keeps `w/WIPI_H <= long/short`, so the
/// height-filling scale is always the smaller one and the picture is at most a
/// fraction of a logical pixel narrower than the surface — never wider, which
/// could only be letterboxed back.
///
/// **The height moves too, so that the scale is a whole number.** Holding the
/// canvas at 240 leaves S25 with 1080/240 = 4.5, and half a pixel is the one
/// thing a nearest-neighbour upscale cannot place: every other canvas row lands
/// on five device rows and the rest on four. Picking the largest integer
/// magnification that still leaves the canvas at least as tall as the authored
/// 240 gives 4 on that phone, hence 585x270, and every canvas pixel is then
/// exactly 4x4 device pixels. With Scale2x in front of it the whole chain is
/// integral — 585x270 -> 1170x540 -> 2340x1080, a clean 2x at the window.
///
/// Changing the height at all is only safe because the guest never branches on
/// one. `Graphic::GetHeight` is called 310 times across 89 functions and
/// `GetHalfHeight` 140 times across 103, and **not one** of those results is
/// compared against a constant — all 27 comparisons in `.text` are against
/// another register, i.e. a runtime clip. The width is the axis with a
/// resolution ladder (`cmp GetWidth(), #240` x10, `#320` x2, `#480` x2), and
/// `long / k` stays on the same rung as the 520 that shipped: 585 and 555 are
/// both above 480. The 240s that *are* hardcoded are the `_ENEMY_G_DATA` stride
/// (a `mul`, nine sites), two `memset` lengths and an HSB hue. ANALYSIS.md §33.
///
/// The integer rule declines rather than distorts. A surface whose long edge
/// divided by `k` would fall below [`WIPI_W`] must not be clamped back up — that
/// would make the canvas wider than `long/scale` and push the picture off the
/// screen instead of letterboxing it. Such a display gets the old fractional
/// scale, which is still uniform and still correct, just not pixel-exact.
pub fn widened_canvas(surface_w: i32, surface_h: i32) -> (i32, i32) {
    let (long, short) = if surface_w >= surface_h {
        (surface_w, surface_h)
    } else {
        (surface_h, surface_w)
    };
    let short = short.max(1);

    let k = (short / WIPI_H).max(1);
    let (w, h) = (long / k, short / k);
    if h >= WIPI_H && (WIPI_W..=1024).contains(&w) {
        return (w, h);
    }

    let w = ((WIPI_H as f64 * long as f64 / short as f64).floor() as i32).clamp(WIPI_W, 1024);
    (w, WIPI_H)
}

/// [`widened_canvas`], narrowed by the player's 해상도 setting.
///
/// Only the **width** moves. The height is what fixes the magnification
/// (`short / h`), so leaving it alone is what keeps every canvas pixel the same
/// whole number of device pixels it was before — the picture does not get
/// smaller or softer, it gets *narrower*, and the shell centres it with black
/// down each side. That is the whole difference between this and scaling the
/// finished picture, which would turn a 4.0 magnification into 3.8.
///
/// The floor is [`WIPI_W`] for the same reason [`widened_canvas`] has one: a
/// canvas narrower than the authored 400 crops the UI instead of framing it.
/// A display small enough for the clamp to bite gets [`ScreenMode::Full`]'s
/// canvas whatever it asked for, which is the honest answer — there is nothing
/// to give back.
///
/// [`ScreenMode::Portrait`] is the exception: its canvas is the guest's own
/// 240x320, whatever the display, because the window it is drawn into is a
/// 3:4 box the shell carves out of an upright screen. See [`portrait`].
pub fn widened_canvas_for(surface_w: i32, surface_h: i32, mode: ScreenMode) -> (i32, i32) {
    let pct = match mode.width_pct() {
        Some(p) => p,
        None => return portrait::CANVAS,
    };
    let (w, h) = widened_canvas(surface_w, surface_h);
    if mode == ScreenMode::Full {
        return (w, h);
    }
    (((w * pct) / 100).clamp(WIPI_W, w), h)
}

/// [`Vm::fit_screen`]'s arithmetic: `(magnification, margin_x, margin_y)` for
/// a `w`x`h` display and a `ww`x`wh` WIPI screen.
fn fit_screen(w: i32, h: i32, ww: i32, wh: i32) -> (f32, i32, i32) {
    let (ww, wh) = (ww.max(1), wh.max(1));
    let (long, short) = if (w >= h) == (ww >= wh) { (w, h) } else { (h, w) };
    let mag = (long as f32 / ww as f32).min(short as f32 / wh as f32);
    (mag, (long - (ww as f32 * mag) as i32) / 2, (short - (wh as f32 * mag) as i32) / 2)
}

/// Real elapsed time as the guest should see it, at `speed` times real speed.
///
/// Three things this has to get right, all of them learned the hard way
/// (ANALYSIS.md §31.9):
///
/// * **`speed == 1.0` takes the old expression, character for character.** The
///   default build must not move by a single rounding step, and `x * 1.0` is
///   not quite that promise once the elapsed value is large.
/// * **`f64`, not `f32`.** `elapsed_ms as f32` quantises to about a quarter of
///   a millisecond after an hour of play, and the guest reads this monotonically
///   — a clock that stutters backwards is worse than a slow one.
/// * **saturating, not wrapping.** The guest treats this as a monotonic
///   `i64`; a wrap would look like the clock going backwards by 585 million
///   years, and the 60-second network watchdog would fire on it.
fn scale_elapsed(elapsed: std::time::Duration, speed: f64) -> i64 {
    let ms = elapsed.as_millis() as i64;
    if speed == 1.0 {
        return ms;
    }
    let scaled = ms as f64 * speed;
    if scaled >= i64::MAX as f64 {
        i64::MAX
    } else if scaled <= 0.0 {
        0
    } else {
        scaled as i64
    }
}

/// `MC_grpFlushLcd` (0x14D898) — the guest's own "put this on the screen".
///
/// Counted unconditionally by [`Vm::boot`], because two different things need
/// the same fact and neither can get it anywhere else:
///
/// * [`hud::Hud`] has to know whether the picture on screen is one that
///   contained the in-game bar, which is not the same question as "did the
///   last `nativeLoop` run" — the guest paces itself and most polls draw
///   nothing;
/// * the device's ten-second line reports it as `repaints/s`, next to a
///   `ticks/s` that counts *host* loop iterations. Reading those two as one
///   number is what made a CPU-bound scene look like it had reached the
///   guest's design ceiling (ANALYSIS.md §31.9).
///
/// The frame path is `TimerCb` -> `CUSTOM_grpFlushLcd` (0x3FCB8) -> here. It
/// is a counter, not a patch and not a hook: the guest runs exactly as
/// shipped, so nothing about the offline A/B changes.
const GUEST_REPAINT: &str = "MC_grpFlushLcd";

/// `BFont::DrawText(int fb, int x, int y, char* s, int align, int colour)` —
/// 0x445E8. Three unrelated features hook this one entry, so it gets a
/// dispatcher rather than a winner.
const DRAW_TEXT_PLAIN: &str = "_ZN5BFont8DrawTextEiiiPcii";

/// Offer the shared `DrawText` entry to each feature that wants a call site
/// in it.
///
/// Each tests the return address first and declines anything that is not its
/// own site, so this is order-independent — and a call made *by* one of them
/// (through [`Vm::call`], with `lr = RETURN_MAGIC`) is declined by all of
/// them, which is what stops the recursion.
///
/// Three arms rather than three `install_hook` calls because
/// [`Vm::install_hook`] refuses a second hook on an address: two features
/// silently sharing one entry, with only one of them ever running, is the
/// failure this makes impossible.
fn draw_text_plain(vm: &mut Vm) -> Result<bool, String> {
    if shop::compact_price_hook(vm)? {
        return Ok(true);
    }
    if shop::revive_nag_hook(vm)? {
        return Ok(true);
    }
    hud::bar_text_hook(vm)
}

/// `Graphic::DrawImage(fb, x, y, GbmImage*, frame)` — the general sprite blit.
/// Two features want one call site each in it: the warehouse's coin icon
/// (hidden offline) and the ten-page tabs.
const DRAW_IMAGE: &str = "_ZN7Graphic9DrawImageEiiiP8GbmImagei";

/// Offer the shared `DrawImage` entry to each feature that owns a call site in
/// it. Same contract as [`draw_text_plain`]: each arm tests its return
/// address and declines everything else.
fn draw_image(vm: &mut Vm) -> Result<bool, String> {
    if vm.cfg.offline_menus && warehouse::hide_coin_icon(vm)? {
        return Ok(true);
    }
    if vm.cfg.ten_pages {
        return tenpages::draw_image_arm(vm);
    }
    Ok(false)
}

/// `Graphic::DrawImageBlending(fb, x, y, GbmImage*, …)` — the HUD's and the
/// ten-page tabs' blit. Two features want it: the ten-page chests (one call
/// site each in four screens) and the 배치변경 capture passes, which leave
/// one HUD piece out at a time.
const DRAW_IMAGE_BLENDING: &str = "_ZN7Graphic17DrawImageBlendingEiiiP8GbmImagehhhi";

fn draw_image_blending(vm: &mut Vm) -> Result<bool, String> {
    if hudlayout::blend_arm(vm)? {
        return Ok(true);
    }
    if vm.cfg.ten_pages {
        return tenpages::dim_tab_arm(vm);
    }
    Ok(false)
}

/// What the host does about the guest's boot-time server dance.
///
/// `StateMainMenu` starts in state 0 (CheckEvent) and 1 (UserLogReg), whose
/// only job is to dial the 2011 servers; the only door onward is the popup
/// *result* of that attempt (ANALYSIS.md §16.3), so simply blocking the
/// network does not speed the boot up — it parks it on a white screen for
/// ever. Skipping means doing what the popup handler would have done.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BootMode {
    /// The guest's own boot, untouched: "Connecting" twice, two error popups
    /// (offline) or a version notice (online), then the intro. Every replay
    /// script under `tools_re/scripts/` and the S9 reference A/B assume this
    /// path, which is why it stays the [`Config`] default.
    Original,
    /// Right after boot the host moves `StateMainMenu` to state 2 (Logo) with
    /// the same two guest calls the popup-result handler at
    /// `StateMainMenu::Process+…` (0x124E24) makes — `LoadLogoSprite(0)` then
    /// `SetState(2)` — so states 0/1 never run and `endProcess` never reaches
    /// `netConnect`. The intro plays unchanged (user decision, 2026-08-18);
    /// online features still work because every mode logs itself in when
    /// opened (§16.7). See [`Vm::try_skip_network`].
    SkipNetwork,
}

pub struct Config {
    /// Contents of `assets/data.vfs.mp3` — the game's whole asset archive,
    /// despite the extension.
    pub vfs: Arc<Vec<u8>>,
    /// Where save files go. The guest builds absolute paths under this.
    pub storage: PathBuf,
    /// Directory the `getAssetSize`/`loadAsset` callbacks read from. In the
    /// original these are `AssetManager.open(name)` against the APK's
    /// `assets/` folder, so this points at the extracted assets (default
    /// `payload/`, which holds `mtouchdevice.txt` and the rest).
    pub assets: PathBuf,
    /// Model name `getModel()` reports. Must be one of `mtouchdevice.txt`'s
    /// four lines for touch to enable — defaults to `SHW-M250S` (Galaxy S).
    pub model: String,
    /// What `CletEntry.getMin()` reports, which the guest asks for as the
    /// `PHONENUMBER` system property and puts in both the SmartReq header and
    /// the login packet (`PacketWrite_Login` subcmd 1).
    ///
    /// On the original this was the handset's own number, and the server keyed
    /// accounts on it. There is no number here, so the Android shell generates
    /// one per installation and stores it beside the save. Empty means "no
    /// identity", and a private server should refuse that rather than merge
    /// every player onto one shared account.
    pub account_id: String,
    /// The remote game server. `Some((host, port))` makes the socket layer
    /// dial a real server (the hero3/hero4 backend); `None` keeps the
    /// deterministic "servers are gone" behaviour used for offline bring-up.
    pub server: Option<(String, u16)>,
    /// With no `server`, answer the guest's connects **in-process** instead of
    /// failing them: each socket gets a [`localserver::Session`], the private
    /// server transcribed to Rust and proven byte-identical to it by
    /// `tests/localserver_ab.rs`. This is what makes the cash shop work with
    /// no network at all. `false` (the default) keeps the dead-server world
    /// every existing replay and reference A/B is built on; a configured
    /// `server` always wins over this flag.
    pub local_server: bool,
    /// Physical surface size in pixels.
    pub screen_w: i32,
    pub screen_h: i32,
    /// The logical screen handed to `nativeInitDisplay`.
    pub wipi_w: i32,
    pub wipi_h: i32,
    /// Use a virtual clock instead of the wall clock. The guest idles on WIPI
    /// timers, so a headless run that does not advance time renders nothing.
    pub virtual_clock: bool,
    /// See [`BootMode`]. Defaults to [`BootMode::Original`] so every existing
    /// replay and reference comparison keeps meaning what it meant.
    pub boot_mode: BootMode,
    /// The cash shop without a network (CLAUDE.md "캐시샵을 로컬로"). When set
    /// and [`Config::server`] is `None`, guest sockets connect to an
    /// in-process server that answers the login/shop conversation
    /// (`localserver::Session`), and the cash-shop price line is drawn struck
    /// through with `0원` beside it (`shop::install_hooks`). Off by default:
    /// every existing replay, reference comparison and `patch.rs` A/B assumes
    /// the unmodified world.
    pub free_shop: bool,
    /// Which `_NET_ITEM_` each shop row grants, while the mapping stays
    /// unconfirmed. See [`shop::ShopGrant`]; empty means purchases complete
    /// with zero records, exactly like the deployed private server.
    pub shop_grants: Vec<shop::ShopGrant>,
    /// How fast guest time runs against real time. 1.0 is the game as
    /// written; see [`Vm::now_ms`] for why this is the only honest lever on
    /// game speed and what it costs.
    pub speed: f64,
    /// Answer the 창고접속 conversation the deployed server never answered, so
    /// that menu opens and each item that needs an operator behind it says so
    /// in the guest's own popup instead of timing out into
    /// `서버상태가 좋지 않습니다 … (302)`.
    ///
    /// Needs [`Config::local_server`] (or [`Config::free_shop`], which implies
    /// it) to have any effect — it is a flag *on* the in-process server, and
    /// without it that server answers exactly the six frames
    /// `hl5_server/server.py` answers, which is what `localserver_ab.rs`
    /// asserts. Off by default for the same reason every other behaviour flag
    /// is: the existing replays and A/Bs assume the unmodified world.
    pub offline_menus: bool,
    /// Stop an action-key release — and the stand that follows an attack /
    /// skill / dash — from ending the walk while the joystick is still held.
    /// See [`input`] for the two hooks and the addresses.
    ///
    /// Off by default for the reason every behaviour flag here is off by
    /// default: this one *does* change what a replay draws, so the 1,241-frame
    /// acceptance A/B has to be able to run without it. The shell turns it on
    /// — it is a fix the user asked for, and there is no configuration in
    /// which the shipped behaviour is the wanted one.
    pub keep_walking: bool,
    /// Hand the guest rewritten probability tables — see [`rates`].
    ///
    /// Off by default for the same reason as [`Config::keep_walking`]: it
    /// changes what a replay draws, so the 1,241-frame acceptance A/B has to be
    /// able to run without it. The shell turns it on.
    pub boost_rates: bool,
    /// The `vanilla` content line — see `BRANCHES.md`.
    ///
    /// `true` makes the `(진)` weapons **unobtainable** without making them
    /// unreadable: the four rows are still written, so a copy someone already
    /// has keeps its name and stats, but drop group 63 and
    /// [`legendaries::extra_endgame_rolls`] point at
    /// [`legendaries::SOURCE_INDEX`] — the shipped lv97 row that copy was made
    /// from — instead of [`legendaries::JIN_INDEX`]. Removing the rows instead
    /// would corrupt those saves — see [`legendaries::install`].
    ///
    /// **The drop *rate* is identical on both lines.** The extra boss roll
    /// happens either way, on the same bosses, in the same band, the same
    /// number of times; only the row it lands on differs. Nothing else in this
    /// shell reads this flag, so no monster's drop rate can differ because of
    /// it.
    ///
    /// Everything else this shell adds is in both lines, because that is what
    /// the operator asked for — the switch is deliberately **one** feature
    /// wide, not a "no custom content" master switch. Whatever else joins it
    /// has to be listed in `BRANCHES.md` and read this flag here.
    ///
    /// Off by default so the acceptance A/B and the probe keep running the
    /// line the shell ships. `build_android.ps1` sets it from
    /// `android_app/flavor.properties`.
    pub vanilla: bool,
    /// Read the player's save and put it in the boot log, base64 encoded —
    /// see [`savedump`].
    ///
    /// Diagnostics only, and never on by default: the release APK is not
    /// `debuggable`, so this is the only way a location-dependent bug report
    /// can be turned into an offline replay. It reads and logs; it never
    /// writes.
    pub dump_save: bool,
    /// Add the cash-shop items the 2011 game never shipped — see
    /// [`cashitems`].
    ///
    /// Off by default, like every other behaviour change here, so the
    /// 1,241-frame A/B can run without it. The shell turns it on.
    pub new_cash_items: bool,
    /// Game ticks per real second, for turning buff durations and buff
    /// cooldowns from the tenths of a second the skill text prints into ticks
    /// — see [`skilltime`]. `0` leaves every timer in the guest's own units.
    ///
    /// Off by default like every other behaviour flag, so the 1,241-frame A/B
    /// can run without it. The shell sets it to its tick cap.
    pub skill_time_hz: u32,
    /// The operator's buff rebalance — twice the duration for the buffs that
    /// cost nothing and recharge in a second, twice SP, cooldown and duration
    /// for every other class buff. See [`skilltime::REBALANCE`].
    ///
    /// Off by default like every other behaviour flag; the shell turns it on
    /// with [`Config::skill_time_hz`], behind the same switch.
    pub buff_rebalance: bool,
    /// How wide a canvas the player asked for — see [`screen`] and
    /// [`widened_canvas_for`].
    ///
    /// This is *not* the switch for the setting's UI; it is the value the
    /// shell already resolved before the guest existed, because
    /// `nativeInitDisplay` runs once and the canvas cannot change afterwards.
    /// [`ScreenMode::Full`] is the canvas every measurement in ANALYSIS.md was
    /// taken with.
    pub screen_mode: screen::ScreenMode,
    /// Draw the 해상도 row on the guest's 환경설정 screen (`screen::install`).
    ///
    /// Off by default for the reason every behaviour flag here is off: it puts
    /// pixels on a screen the 1,241-frame A/B does not visit today, and the
    /// A/B has to be able to run without it. The shell turns it on.
    pub screen_option: bool,
    /// Whether the shell runs the canvas through Scale2x on its way to the
    /// window right now — the 화면필터 row's initial value. The row toggles
    /// [`Vm::scale2x`] and the shell picks the change up through
    /// [`Vm::take_scale2x_change`]; the probe has no presenter, so here it is
    /// only a value to show.
    pub scale2x: bool,
    /// Accept gamepad input, and annotate the field HUD with which pad button
    /// fires which control — see [`gamepad`] and [`padhud`].
    ///
    /// Off by default like every other behaviour flag, so the 1,241-frame A/B
    /// can run without it: with this clear the observer hook is not installed,
    /// no badge is painted and `Vm::pad_*` has nowhere to send anything. The
    /// shell turns it on.
    pub gamepad: bool,
    /// The `*아바타` button on the character page and the wardrobe behind it
    /// — see [`avatar`].
    ///
    /// Off by default like every other behaviour flag, so the 1,241-frame A/B
    /// can run without it: with this clear no hook is installed and the hero
    /// is dressed by its equipment alone. The shell turns it on.
    pub avatar: bool,
    /// The 공용창고 at ten pages of sixteen instead of five — see
    /// [`tenpages`]. Rewrites the warehouse object's layout in guest memory
    /// (one immediate per instruction, all checked first) and redraws the
    /// page tabs as two numbered columns.
    ///
    /// Off by default like every other behaviour flag, so the 1,241-frame A/B
    /// can run without it: with this clear not one guest word is written and
    /// no hook is installed. The shell turns it on.
    pub ten_pages: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vfs: Arc::new(Vec::new()),
            storage: PathBuf::from("out/storage"),
            assets: PathBuf::from("payload"),
            model: "SHW-M250S".into(),
            account_id: String::new(),
            server: None,
            local_server: false,
            // Physical == logical so a headless frame is 1:1 with the WIPI
            // canvas and magnification is 1.0 — the cleanest thing to diff
            // against the S9 reference captures.
            screen_w: WIPI_W,
            screen_h: WIPI_H,
            wipi_w: WIPI_W,
            wipi_h: WIPI_H,
            speed: 1.0,
            offline_menus: false,
            keep_walking: false,
            boost_rates: false,
            vanilla: false,
            dump_save: false,
            new_cash_items: false,
            skill_time_hz: 0,
            buff_rebalance: false,
            screen_mode: screen::ScreenMode::Full,
            screen_option: false,
            scale2x: true,
            gamepad: false,
            avatar: false,
            ten_pages: false,
            virtual_clock: true,
            boot_mode: BootMode::Original,
            free_shop: false,
            shop_grants: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Loaded,
    CtorsRun,
    Running,
    Exited,
}

/// CLET event types, as `CletEntry` passes them to `nativeKeyHandleEvent` /
/// `nativeTouchHandleEvent`. `onHandleEvent` (0x9c20c) switches on
/// `type - 2` with a 20-entry jump table, and only these five reach a state:
/// 2 and 3 to the state's key handlers, 19/20/21 to its touch handlers.
pub const EV_KEY_PRESSED: u32 = 2;
pub const EV_KEY_RELEASED: u32 = 3;
pub const EV_TOUCH_PRESSED: u32 = 19;
pub const EV_TOUCH_RELEASED: u32 = 20;
pub const EV_TOUCH_MOVED: u32 = 21;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Touch {
    Pressed,
    Released,
    Moved,
}

/// WIPI key codes, from `WIPIConstant.getWIPIKeyCode`'s sparse-switch: the
/// five d-pad keys and BACK are the only Android key codes it translates, and
/// everything else becomes 0.
pub fn wipi_key(name: &str) -> Option<i32> {
    Some(match name {
        "up" => -1,
        "down" => -2,
        "left" => -3,
        "right" => -4,
        "select" | "ok" | "center" => -5,
        "back" | "clear" => -16,
        _ => return None,
    })
}

pub struct Vm {
    pub cpu: Cpu,
    pub mem: GuestMem,
    pub heap: Heap,
    pub elf: Elf,
    pub cfg: Config,
    /// File descriptors, stdio streams and the fake socket layer.
    pub files: files::Files,
    /// GLES capture state and the CPU-blitted host framebuffer.
    pub gl: gl::GlState,
    /// JNI handle registry (interned class/method/field lookups).
    pub jni: jni::JniState,
    /// Name→id bijection behind `getUniqueNumberFromStrings`, mirroring the
    /// Java `MidasAssetManager` hashtable.
    pub asset_ids: Vec<String>,
    /// Clip table behind the `CletEntry` audio callbacks. See `audio.rs` —
    /// the host records what to play and where its bytes are; the decoding is
    /// the platform's, exactly as in the original.
    pub audio: audio::Audio,
    /// Set when the guest calls `CletEntry.exitApp`.
    pub exit_requested: bool,
    /// [`BootMode::SkipNetwork`] asked for a jump that could not happen yet
    /// (the `StateMainMenu` object was not constructed when [`Vm::boot`]
    /// finished). [`Vm::frame`] retries before each `nativeLoop` until it
    /// lands or [`SKIP_GIVE_UP_FRAMES`] passes. See [`Vm::try_skip_network`].
    skip_pending: bool,
    /// Frames spent with `skip_pending` set, for the give-up bound.
    skip_tries: u32,
    /// A carrier payment is waiting to be reported back to the guest.
    /// See [`Vm::frame`] and `cletentry.rs`'s `requestMTX`.
    pub pending_mtx_result: bool,

    /// Guest addresses served by a host reimplementation instead of being
    /// interpreted. See `patch.rs` — this is where the frame budget is won.
    pub patches: std::collections::HashMap<u32, patch::PatchFn>,
    /// One bit per instruction word from [`Elf::base`], set where a patch is
    /// installed — **or a behaviour hook**. [`Vm::run`] consults the maps for
    /// **every** interpreted instruction, and a `HashMap` lookup there is a
    /// SipHash of the PC per instruction — pure dispatch overhead on the
    /// hottest loop in the program. The bitmap answers "nothing here", the
    /// common case for all but a handful of addresses, with one shift, one
    /// load and one mask; a `HashMap` is only consulted on a set bit. Empty
    /// when neither map has an entry (`--no-patch` with no `--free-shop`),
    /// which fails the bounds test immediately.
    ///
    /// It covers both maps because a hook on a font routine is exactly as hot
    /// as a patch: guarding the hook map with `!is_empty()` instead put the
    /// per-instruction hash back the moment `--free-shop` was on.
    dispatch_filter: Vec<u64>,
    /// How many times a patched function has run, and how many times one
    /// declined and let the interpreter do it. Reported by the probe: a patch
    /// that never fires is useless, and one that declines most of its calls is
    /// covering the wrong variant.
    pub patch_calls: u64,
    pub patch_declined: u64,

    /// Guest addresses served by a **behaviour** hook — code that changes what
    /// the guest does, as opposed to [`Vm::patches`], whose contract is
    /// byte-identical output. Same dispatch, separate map, so the patch A/B
    /// keeps proving what it proves.
    ///
    /// Features live here, each empty without its installer:
    /// the free-shop price display and purchase modal (`shop::install_hooks`,
    /// [`Config::free_shop`]), the 거래용 화폐 controls
    /// (`warehouse::install_hooks`, [`Config::offline_menus`]), the widened
    /// canvas's in-game bar (`hud::install`, which installs nothing when the
    /// canvas is 400 wide), the walk-through-attack fix
    /// (`input::install`, [`Config::keep_walking`] — two hooks), and the
    /// `(진)` extra boss drop (`legendaries::install_hooks`).
    pub hooks: std::collections::HashMap<u32, patch::PatchFn>,
    /// How many times a behaviour hook took a call. A hook that never fires
    /// is a feature that silently is not there — the probe prints this.
    pub hook_calls: u64,

    /// Guest entry points being **counted**, by address: symbol name and how
    /// many times the interpreter has arrived there. Neither a patch nor a
    /// hook — nothing about the guest changes — so a counter is installable
    /// on the shipping path and the offline A/B still means what it meant.
    ///
    /// It exists because a total does not say where the money went. A static
    /// main menu that changes not one pixel still spends 46–68% of the wall
    /// clock inside `Vm::frame` (ANALYSIS.md §31.3), and "empty tick" versus
    /// "the guest really did repaint" are two different bills wanting two
    /// different fixes. `MC_grpFlushLcd` is the one that tells them apart.
    ///
    /// Dispatch is the same per-word bitmap [`Vm::patches`] and [`Vm::hooks`]
    /// use, so an empty map costs one bounds test per instruction — the test
    /// that was already there.
    probes: std::collections::BTreeMap<u32, (String, u64)>,
    /// Free-shop state: the grant table the in-process private server answers
    /// purchases from, and the price hook's one guest-side allocation.
    pub shop: shop::ShopHost,
    /// Row-to-effect map for the added cash items. See [`cashitems`].
    pub(crate) cash: cashitems::CashHost,
    /// 정체모를 괴수's drop, which the story scene used to eat. See
    /// [`bossdrop`].
    pub(crate) bossdrop: bossdrop::BossDrop,
    /// The stranded drop-table slots this build will roll, already checked
    /// against the shipped table. Empty means the table moved and nothing is
    /// rolled. See [`lostdrops`].
    pub(crate) lostdrops: Vec<[u8; 5]>,
    /// The 공용창고, shared by every in-process server session this run makes.
    ///
    /// One per `Vm` and not one per session, because the guest opens a fresh
    /// socket for each visit to that menu and a warehouse that forgot itself
    /// between visits would be no warehouse at all. Behind a lock so the
    /// handle can be cloned into sessions without borrowing the `Vm`; the
    /// contention is nil (one thread, a few frames a session).
    pub warehouse: std::sync::Arc<std::sync::Mutex<warehouse::Warehouse>>,
    /// The ten-page tabs' labels and counters. See [`tenpages`].
    pub pages10: tenpages::TenPages,
    /// Addresses the font-pixel port reads, resolved once by
    /// [`Vm::install_patches`]. `None` until then, and `None` for ever if this
    /// binary is missing one of them — in which case that port declines and
    /// the guest's own font writer runs. See `patch::FntGlobals`.
    pub(crate) fnt: Option<patch::FntGlobals>,
    /// The host `startDes`: whether each direction has been proven against the
    /// guest's own code, and how many calls it has served. See [`des`].
    pub des: des::Des,
    /// Canvas state: how far the in-game top bar moves, and where it landed.
    /// See [`hud`].
    pub hud: hud::Hud,
    /// Gamepad state: the mapping in force, what the pad is holding, the
    /// queue of edges the guest has not been handed yet, and the field HUD's
    /// rects as the guest last drew them. See [`gamepad`].
    pub pad: gamepad::Pad,
    /// The 해상도 row on 환경설정: what the player has chosen, what this
    /// process is actually running, and where the three letters were drawn.
    /// See [`screen`].
    pub screen: screen::Screen,
    /// The 환경설정 list as the host shows it (fifteen rows). See [`options`].
    pub options: options::Options,
    /// The three save rows' conversation: what the shell's document screens
    /// are asked to do and what they answered. See [`savexfer`].
    pub savexfer: savexfer::SaveXfer,
    /// The 패드키 row and its mapping screen. See [`padmap`].
    pub padmap: padmap::PadMap,
    /// The 배치변경 row: the field HUD's layout in force, its hooks' state,
    /// and the editor. See [`hudlayout`] and [`hudedit`].
    pub hudlayout: hudlayout::HudLayout,
    /// The display filter the 화면필터 row believes is in force, and whether
    /// it changed since the shell last asked.
    pub scale2x: bool,
    pub(crate) scale2x_dirty: bool,
    /// The wardrobe: the catalogue, what each save slot wears, and whether the
    /// page is up. See [`avatar`].
    pub avatar: avatar::Avatar,
    /// `guest_repaints()` as of the start of the last [`Vm::frame`], so that
    /// frame can be classified as "painted" or "found nothing due".
    repaints_seen: u64,

    /// Display fit, as `CletEntry.adjustAutoScreen` computes it and
    /// `nativeInitDisplay` is told it. Set by [`Vm::boot`]; the shell needs
    /// the same three numbers to map touches back.
    pub magnification: f32,
    pub margin_x: i32,
    pub margin_y: i32,

    /// Trampoline id -> imported symbol name, in the order the loader met them.
    pub imports: Vec<String>,
    /// `(address, name)` for every defined symbol, sorted, so [`Vm::describe`]
    /// is a binary search rather than a scan of 3,400 hash entries. Fault
    /// reports and traces both call it, and a trace calls it per HLE hit.
    syms_by_addr: Vec<(u32, String)>,

    pub phase: Phase,
    pub clock_ms: i64,
    clock_origin: std::time::Instant,

    /// Guest instructions retired, and HLE calls served.
    pub steps: u64,
    pub hle_calls: u64,
    pub log: Vec<String>,
    /// Log every HLE call with its arguments and caller. Driven by `HL5_TRACE=1`
    /// so a failing device build can be turned up without a rebuild.
    pub trace: bool,
    /// Guest addresses to log on entry, and the name to log them under.
    ///
    /// The HLE trace only sees the boundary, so a guest that is busy failing
    /// *inside* itself — asking the VFS for a file that is not there, once per
    /// tick — shows up in it as an unattributed `fclose` storm and nothing
    /// else. Watching a named guest function turns that back into a sentence.
    /// Empty by default; the check is one `is_empty` per instruction.
    pub watch: std::collections::HashMap<u32, String>,
    /// Guest PC histogram, sampled every [`PROFILE_PERIOD`] instructions.
    /// `None` unless profiling was asked for.
    pub profile: Option<std::collections::HashMap<u32, u64>>,
    /// Instructions left before the next sample.
    ///
    /// A countdown, not `steps % PERIOD`: a 64-bit modulo costs tens of cycles
    /// and this runs once per guest instruction.
    profile_countdown: u64,
    /// Every load and store into a watched heap object, by PC. `None` except
    /// under the probe's `--memwatch` — see [`memwatch`].
    pub memwatch: Option<Box<memwatch::MemWatch>>,
}

/// Instructions between profiler samples. Large enough that the sampling cost
/// disappears, small enough that a hot loop of a few hundred instructions still
/// shows up.
const PROFILE_PERIOD: u64 = 997;

/// Stack words `--watch` prints alongside r0..r3. Six covers every guest
/// function this project has needed to read so far; the widest is
/// `AndroidService::putClipFromBuffer` with eleven parameters.
const WATCH_STACK_ARGS: u32 = 6;

/// The clock a virtual-clock run starts at. Fixed so a dumped frame is
/// comparable between runs.
const FIXED_CLOCK_MS: i64 = 1_700_000_000_000;

/// Where `StateMainMenu`'s instance pointer lives inside the guest's `gv`
/// global. Read-only from the host; the jump itself is made of guest calls.
const GV_STATE_MAIN_MENU: u32 = 0x27c;

/// How many frames [`BootMode::SkipNetwork`] keeps retrying before it
/// concludes the state object is never coming and lets the original boot run.
/// `StateMainMenu` is constructed during `nativeStartApp`, so in practice the
/// jump lands before the first frame; state 0 does not touch the network until
/// its 17th tick, so even a late landing changes nothing the player sees. The
/// bound only exists so a surprise cannot leave a stale retry in every frame
/// for ever.
const SKIP_GIVE_UP_FRAMES: u32 = 300;

impl Vm {
    pub fn new(so: &[u8], cfg: Config) -> Result<Self, String> {
        let mut mem = GuestMem::new();

        // Imports are discovered during load; each gets a trampoline word in
        // the HLE window and the loader points the guest's slot there. Data
        // imports are handed real storage instead — a trampoline would be read
        // as a value.
        let mut imports: Vec<String> = Vec::new();
        let data_syms = hle::data_import_addr();
        let elf = elf::load(so, IMAGE_BASE, &mut mem, |name| {
            if let Some(addr) = data_syms(name) {
                return Some(addr);
            }
            let idx = match imports.iter().position(|n| n == name) {
                Some(i) => i,
                None => {
                    imports.push(name.to_string());
                    imports.len() - 1
                }
            };
            (imports.len() <= hle::TRAMP_JNIENV).then(|| hle::tramp_addr(idx))
        })?;

        if imports.len() > hle::TRAMP_JNIENV {
            return Err(format!(
                "{} imports but only {} trampoline slots reserved for them",
                imports.len(),
                hle::TRAMP_JNIENV
            ));
        }

        let mut syms_by_addr: Vec<(u32, String)> =
            elf.symbols.iter().map(|(n, &a)| (a, n.clone())).collect();
        syms_by_addr.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        let gl = gl::GlState::new(cfg.screen_w, cfg.screen_h);
        // Before the grant table, not after: `GrantTable::build` parses
        // `item_18.dat` out of the archive and refuses any catalogue row whose
        // `a` is past the row count it found, so a new item added later would
        // list in the shop and be refused at grant time. See `cashitems`.
        let extended = if cfg.new_cash_items { cashitems::build(&cfg.vfs) } else { None };
        let shop = shop::ShopHost::new(
            cfg.free_shop,
            &cfg.shop_grants,
            &cfg.vfs,
            // `item18_grant`, not `item18`: the guest's copy rewrites the usage
            // class so old random boxes can be discarded, and the grant table keys
            // its reroll off exactly that byte. See `cashitems`.
            extended.as_ref().map(|e| (e.item18_grant.as_slice(), e.catalog.as_slice())),
        );
        let cash = extended
            .as_ref()
            .map(|e| cashitems::CashHost::new(e.rows.clone()))
            .unwrap_or_default();
        // Read even when no in-process server will run: it is one small file,
        // and a boot log that says how many slots are in use is the first
        // thing anyone debugging a missing item will want.
        // Ten pages offered when the layout rewrite is on; the file keeps
        // whatever it holds either way (see `Warehouse::load_offering`).
        let offered = if cfg.ten_pages {
            tenpages::STORAGE_SLOTS
        } else {
            warehouse::DEFAULT_CAPACITY
        };
        let (wh, wh_logs) = warehouse::Warehouse::load_offering(&cfg.storage, offered);
        // Written before the guest boots, because `StaticUtil::LoadRes` reads
        // it the first time a refine or orb screen is opened and never looks
        // again.
        // Hell pair-B result indices are a shipped typo, not a rate policy —
        // they are rewritten even when `boost_rates` is off. `install` already
        // applies the same remap on `smith_2`, so the off path is the slim
        // installer that does not also lift 조합 percents.
        let mut rate_logs = if cfg.boost_rates {
            rates::install(&cfg.vfs, &cfg.storage)
        } else {
            rates::install_hell_legendaries(&cfg.vfs, &cfg.storage)
        };
        // Monster drop rates are the game's own on **both** content lines
        // (operator, 2026-09-07). This runs whatever `boost_rates` says,
        // because what it removes was written by a build already in the field
        // and `LoadRes` would keep preferring it. See
        // [`rates::restore_shipped_drop_bands`].
        rate_logs.extend(rates::restore_shipped_drop_bands(&cfg.storage));
        // The buff rebalance's SP and cooldown bases and the real-second text
        // literals, or — when off — the removal of a shadow an earlier boot
        // left. See `skilltime`.
        rate_logs.extend(skilltime::install_shadows(
            &cfg.vfs,
            &cfg.storage,
            cfg.buff_rebalance,
            cfg.skill_time_hz,
        ));
        // (진) copies of the four Hell-endgame weapons, plus drop-table
        // group 63. Always on: this is new content, not a rate policy, and
        // the extra roll declines unless the shadows actually landed.
        let (jin_logs, _jin_ok) = legendaries::install(&cfg.vfs, &cfg.storage, !cfg.vanilla);
        // The marked 정체모를 괴수 row of each difficulty table, so the boss's
        // drop can be paid even if the map spawns the table's dead twin. Read
        // here because `cfg.vfs` is moved into the Vm below. See [`bossdrop`].
        let (bossdrop, bossdrop_logs) = bossdrop::BossDrop::new(&cfg.vfs);
        // The twenty equipment rows the shipped tables strand behind a
        // zero-width band. Checked against the shipped drop table here, for
        // the same reason as above: `cfg.vfs` is moved into the Vm below.
        let (lostdrops, lostdrop_logs) = {
            let dt = shop::vfs_entry(&cfg.vfs, legendaries::DROP_TABLE_PATH).unwrap_or(&[]);
            let (ok, bad) = lostdrops::verify(dt);
            let mut logs = bad;
            logs.push(format!(
                "lostdrops: {}/{} stranded slots armed ({} rows), ~0.25%/kill",
                ok.len(),
                lostdrops::LOST_SLOTS.len(),
                ok.iter().map(|e| (e[3], e[4])).collect::<std::collections::BTreeSet<_>>().len(),
            ));
            (ok, logs)
        };
        // Diagnostics: read the player's save into the boot log so a
        // location-dependent bug on a non-debuggable release build can be
        // replayed offline. Off unless asked for — see [`savedump`].
        let save_logs =
            if cfg.dump_save { savedump::dump(&cfg.storage) } else { Vec::new() };
        // `cfg` is moved into the Vm below, so anything the post-construction
        // logging needs has to be taken now.
        let cfg_storage = cfg.storage.clone();
        let hud = hud::Hud::new(cfg.wipi_w);
        let screen = screen::Screen::new(cfg.screen_mode);
        let cfg_scale2x = cfg.scale2x;
        let (avatar, avatar_logs) = if cfg.avatar {
            avatar::Avatar::new(&cfg.vfs, &cfg.storage)
        } else {
            avatar::Avatar::new(&[], &cfg.storage)
        };
        let mut vm = Self {
            cpu: Cpu::new(),
            mem,
            heap: Heap::new(HEAP_BASE, HEAP_SIZE),
            elf,
            files: files::Files::new(),
            gl,
            jni: jni::JniState::default(),
            asset_ids: Vec::new(),
            audio: audio::Audio::new(),
            exit_requested: false,
            skip_pending: false,
            skip_tries: 0,
            pending_mtx_result: false,
            patches: Default::default(),
            dispatch_filter: Vec::new(),
            patch_calls: 0,
            patch_declined: 0,
            hooks: Default::default(),
            fnt: None,
            des: Default::default(),
            hud,
            pad: gamepad::Pad::default(),
            screen,
            options: options::Options::default(),
            savexfer: savexfer::SaveXfer::default(),
            padmap: padmap::PadMap::default(),
            hudlayout: hudlayout::HudLayout::default(),
            scale2x: cfg_scale2x,
            scale2x_dirty: false,
            avatar,
            repaints_seen: 0,
            hook_calls: 0,
            probes: Default::default(),
            shop,
            cash,
            bossdrop,
            lostdrops,
            warehouse: std::sync::Arc::new(std::sync::Mutex::new(wh)),
            pages10: Default::default(),
            magnification: 1.0,
            margin_x: 0,
            margin_y: 0,
            imports,
            syms_by_addr,
            phase: Phase::Loaded,
            clock_ms: FIXED_CLOCK_MS,
            clock_origin: std::time::Instant::now(),
            steps: 0,
            hle_calls: 0,
            log: Vec::new(),
            trace: std::env::var("HL5_TRACE").is_ok_and(|v| v != "0"),
            watch: Default::default(),
            profile: None,
            profile_countdown: PROFILE_PERIOD,
            memwatch: None,
            cfg,
        };
        vm.log.extend(wh_logs);
        vm.log.extend(rate_logs);
        // Which content line this binary is. Printed first so a logcat from a
        // handed-out APK says which of the two it is without unzipping it
        // (`BRANCHES.md`); the APK filename says the same thing.
        vm.log.push(format!(
            "flavor: {}",
            if vm.cfg.vanilla { "vanilla — (진) kept but unobtainable" } else { "main — all content" }
        ));
        vm.log.extend(jin_logs);
        vm.log.extend(bossdrop_logs);
        vm.log.extend(lostdrop_logs);
        vm.log.extend(save_logs);
        if vm.cfg.avatar {
            vm.log.extend(avatar_logs);
        }
        if let Some(e) = &extended {
            vm.log.extend(e.logs.clone());
            vm.log.extend(e.write_shadows(&cfg_storage));
        }

        // System mode, ARM state, interrupts masked. The guest never touches
        // CPSR, but the interpreter reads the mode bits on every access.
        vm.cpu.reg_set(Mode::User, reg::CPSR, 0x1f);
        vm.cpu.reg_set(Mode::User, reg::SP, STACK_BASE + STACK_SIZE - 16);

        vm.build_jni();

        Ok(vm)
    }

    pub fn symbol(&self, name: &str) -> Option<u32> {
        self.elf.symbols.get(name).copied()
    }

    // ---- clock ------------------------------------------------------------

    /// The guest's wall clock, in milliseconds.
    ///
    /// # This is the single lever on game speed
    ///
    /// `ftime` is the **only** wall-clock source the guest has: of the 77
    /// undefined dynsyms exactly one is a time function, its only caller is
    /// `MIDASKernelManager::currentTime` (0x1522E4), and all 43
    /// `MC_knlCurrentTime` sites funnel into that. No JNI entry point and no
    /// `CletEntry` callback carries a timestamp. Host-side that is `hle.rs`'s
    /// `ftime` arm, and it reads this function — so multiplying here scales
    /// **everything** the guest paces on time, in phase: the timer that drives
    /// a tick and the code that integrates milliseconds both move by the same
    /// factor. Patching the guest's fps table instead would speed the tick up
    /// and leave `ParticleMgr::process` (0xD1A74) integrating real
    /// milliseconds, i.e. effects running at 1x over a 2x game — which is the
    /// symptom this is meant to cure, not cause (ANALYSIS.md §31.2).
    ///
    /// # Why only the wall-clock branch scales
    ///
    /// In virtual-clock mode the caller owns the clock: [`Vm::advance_time`]
    /// is fed the guest's *own* millisecond counts (`files.rs` advances it by
    /// a `select` timeout), so scaling there would corrupt those. A headless
    /// run wanting a faster guest advances by more per frame instead.
    ///
    /// **So the probe cannot exercise the line below**, and there is therefore
    /// no byte-identical acceptance test for this feature; [`scale_elapsed`]
    /// carries the part that can be tested. The honest evidence is a device
    /// log whose tick rate actually reaches `27.8 x speed`.
    pub fn now_ms(&self) -> i64 {
        if self.cfg.virtual_clock {
            self.clock_ms
        } else {
            self.clock_ms + scale_elapsed(self.clock_origin.elapsed(), self.cfg.speed)
        }
    }

    /// Advance the virtual clock. No effect in wall-clock mode.
    ///
    /// A headless run that never calls this renders nothing at all: the guest
    /// idles on WIPI timers, so with time stopped it does exactly what it is
    /// told and draws no frame. That looks like a hang and is not one.
    pub fn advance_time(&mut self, ms: i64) {
        if self.cfg.virtual_clock {
            self.clock_ms += ms;
        }
    }

    // ---- guest execution ---------------------------------------------------

    /// Call a guest function and return r0. Re-entrant: an HLE import that has
    /// to call back into the guest (`qsort`'s comparator does) can use this
    /// from inside a `run`.
    pub fn call(&mut self, func: u32, args: &[u32]) -> Result<u32, String> {
        self.call2(func, args).map(|(r0, _)| r0)
    }

    /// Like [`Vm::call`], but returns the (r0, r1) pair — soft-float doubles
    /// and other 64-bit values come back split across both.
    pub fn call2(&mut self, func: u32, args: &[u32]) -> Result<(u32, u32), String> {
        let saved = self.cpu;

        let mut sp = self.cpu.reg_get(Mode::User, reg::SP) & !7;
        if args.len() > 4 {
            let extra = args.len() - 4;
            sp -= ((extra * 4 + 7) & !7) as u32;
            for (i, a) in args[4..].iter().enumerate() {
                self.mem.write_u32(sp + (i * 4) as u32, *a);
            }
        }
        for (i, a) in args.iter().take(4).enumerate() {
            self.cpu.reg_set(Mode::User, i as u8, *a);
        }
        self.cpu.reg_set(Mode::User, reg::SP, sp);
        self.cpu.reg_set(Mode::User, reg::LR, RETURN_MAGIC);
        self.cpu.reg_set(Mode::User, reg::PC, func);

        let r = self.run();
        let ret = (self.cpu.reg_get(Mode::User, 0), self.cpu.reg_get(Mode::User, 1));
        self.cpu = saved;
        r?;
        Ok(ret)
    }

    /// Run until PC reaches [`RETURN_MAGIC`].
    fn run(&mut self) -> Result<(), String> {
        // The ceiling only exists so a guest that wedges surfaces as an error
        // instead of hanging the UI thread.
        const BUDGET: u64 = 2_000_000_000;
        let mut budget = BUDGET;

        loop {
            let pc = self.cpu.reg_get(Mode::User, reg::PC);
            if pc == RETURN_MAGIC {
                return Ok(());
            }
            if let Some(id) = hle::tramp_index(pc) {
                self.dispatch(id)?;
                continue;
            }
            // The bitmap answers "is a patch or hook installed at this PC"
            // without hashing — see `dispatch_filter`. A PC below the image
            // base wraps to a huge index and fails the bounds test, same as
            // one past the last entry; only a set bit pays for a `HashMap`
            // lookup.
            let word = pc.wrapping_sub(self.elf.base) >> 2;
            let slot = (word >> 6) as usize;
            if slot < self.dispatch_filter.len()
                && self.dispatch_filter[slot] & (1u64 << (word & 63)) != 0
            {
                if let Some(f) = self.patches.get(&pc).copied() {
                    if f(self)? {
                        self.patch_calls += 1;
                        // Return to the caller the way the guest's own epilogue
                        // would. A patch that needed to re-enter the guest (a
                        // pixel-op callback) went through `Vm::call`, which
                        // saves and restores the CPU, so `lr` is still the
                        // caller's.
                        let lr = self.cpu.reg_get(Mode::User, reg::LR);
                        self.cpu.reg_set(Mode::User, reg::PC, lr);
                        continue;
                    }
                    // Declined: fall through and interpret the original. The
                    // count is what says whether a port is actually covering
                    // the variants this game uses.
                    self.patch_declined += 1;
                }
                if let Some(f) = self.hooks.get(&pc).copied() {
                    if f(self)? {
                        self.hook_calls += 1;
                        let lr = self.cpu.reg_get(Mode::User, reg::LR);
                        self.cpu.reg_set(Mode::User, reg::PC, lr);
                        continue;
                    }
                    // Declined — for a behaviour hook that is the *common*
                    // case (the free-shop hook sits on a font call and takes
                    // only one call site), so no counter for it.
                }
                // Pure measurement: count the arrival and interpret the
                // function exactly as shipped. See [`Vm::probes`].
                if let Some(c) = self.probes.get_mut(&pc) {
                    c.1 += 1;
                }
            }

            if !self.watch.is_empty() {
                if let Some(name) = self.watch.get(&pc).cloned() {
                    let lr = self.cpu.reg_get(Mode::User, reg::LR);
                    let from = self.describe(lr);
                    let args: Vec<String> =
                        (0..4).map(|i| self.render_arg(self.cpu.reg_get(Mode::User, i))).collect();
                    // The watch fires on the function's first instruction, so
                    // the prologue has not moved sp yet and AAPCS's stack
                    // arguments — everything past the fourth word, which is
                    // most of the interesting ones in this guest — start at
                    // sp+0. Without these a `putSoundClip(handle, offset,
                    // length, type)` shows only the handle.
                    let sp = self.cpu.reg_get(Mode::User, reg::SP);
                    let stack: Vec<String> = (0..WATCH_STACK_ARGS)
                        .map(|i| self.render_arg(self.mem.read_u32(sp + i * 4)))
                        .collect();
                    self.log.push(format!(
                        "{name}({}) [sp: {}] <- {from}",
                        args.join(", "),
                        stack.join(", ")
                    ));
                }
            }

            let stepped = match self.memwatch.as_deref_mut() {
                None => self.cpu.step(&mut self.mem),
                Some(w) => {
                    w.at_pc(pc, self.cpu.reg_get(Mode::User, 0));
                    let mut m = memwatch::Watched { mem: &mut self.mem, w, pc };
                    self.cpu.step(&mut m)
                }
            };
            if !stepped {
                let what = self.describe(pc);
                let ctx = self.context(pc);
                return Err(format!("undefined instruction at {pc:#010x} ({what})\n{ctx}"));
            }
            self.steps += 1;

            if self.profile.is_some() {
                self.profile_countdown -= 1;
                if self.profile_countdown == 0 {
                    self.profile_countdown = PROFILE_PERIOD;
                    let here = self.cpu.reg_get(Mode::User, reg::PC);
                    if let Some(p) = self.profile.as_mut() {
                        *p.entry(here).or_insert(0) += 1;
                    }
                }
            }

            // Check before taking: `take` writes the Option back every
            // instruction, a read does not.
            if self.mem.fault.is_some() {
                let f = self.mem.take_fault().expect("just checked");
                let what = self.describe(pc);
                let ctx = self.context(pc);
                return Err(format!(
                    "guest {} of {} bytes at {:#010x} from {what} — outside the mapped window\n{ctx}",
                    if f.write { "write" } else { "read" },
                    f.width,
                    f.addr,
                ));
            }

            budget -= 1;
            if budget == 0 {
                return Err(format!("guest ran {BUDGET} instructions without returning"));
            }
        }
    }

    // ---- diagnostics -------------------------------------------------------

    /// Arrange for [`Vm::watch`] to log every call to `sym`. Returns false if
    /// the binary has no such symbol, so a typo is reported rather than
    /// silently watching nothing.
    pub fn watch_symbol(&mut self, sym: &str) -> bool {
        // A bare address is allowed too. Inside a 7.9 KB blitter the thing
        // worth watching is often *which inner loop* the dispatch picked, and
        // those branch targets have no symbols of their own — only the
        // enclosing function does.
        if let Some(hex) = sym.strip_prefix("0x") {
            if let Ok(a) = u32::from_str_radix(hex, 16) {
                let addr = if a < crate::mem::IMAGE_BASE { a + self.elf.base } else { a };
                self.watch.insert(addr, format!("{sym} ({})", self.describe(addr)));
                return true;
            }
        }
        match self.symbol(sym) {
            Some(a) => {
                self.watch.insert(a, sym.to_string());
                true
            }
            None => false,
        }
    }

    /// Count every entry into `sym`, changing nothing about how it runs.
    ///
    /// Returns false if the binary has no such symbol — a typo reports itself
    /// instead of silently counting nothing, the same contract
    /// [`Vm::watch_symbol`] has. Unlike a watch this costs nothing per call
    /// (no formatting, no log line), so it is meant to be left on in the
    /// shipping shell: the point is a rate in the device's ten-second stat
    /// line, over hours, not a trace.
    ///
    /// Rebuilds the dispatch bitmap itself, so it is safe to call before or
    /// after [`Vm::boot`].
    pub fn count_symbol(&mut self, sym: &str) -> bool {
        // A bare address is allowed too, as in `Vm::watch_symbol`: inside a
        // 7.9 KB blitter the interesting thing is often *which inner loop* the
        // dispatcher picked, and those branch targets have no symbols of their
        // own.
        let addr = if let Some(hex) = sym.strip_prefix("0x") {
            match u32::from_str_radix(hex, 16) {
                Ok(a) if a < crate::mem::IMAGE_BASE => a + self.elf.base,
                Ok(a) => a,
                Err(_) => return false,
            }
        } else {
            match self.symbol(sym) {
                Some(a) => a,
                None => return false,
            }
        };
        // Same guard the two hook installers apply: the bitmap indexes ARM
        // words above the image base, so anything else would set a bit
        // nothing ever tests and the counter would read zero for ever.
        if addr < self.elf.base || addr % 4 != 0 {
            return false;
        }
        self.probes.entry(addr).or_insert_with(|| (sym.to_string(), 0));
        self.rebuild_dispatch_filter();
        true
    }

    /// Every installed counter, as (symbol, entries), in address order.
    pub fn probe_counts(&self) -> Vec<(&str, u64)> {
        self.probes.values().map(|(n, c)| (n.as_str(), *c)).collect()
    }

    /// One counter by symbol name, or 0 if it was never installed.
    pub fn probe_count(&self, sym: &str) -> u64 {
        self.probes.values().find(|(n, _)| n == sym).map_or(0, |(_, c)| *c)
    }

    /// How many times the guest has repainted the screen — `MC_grpFlushLcd`
    /// entries. See `GUEST_REPAINT`; this is the guest's own rate, and it is
    /// nothing like the rate this loop is driven at.
    pub fn guest_repaints(&self) -> u64 {
        self.probe_count(GUEST_REPAINT)
    }

    /// One argument register, as hex plus the C string it points at when it
    /// plausibly points at one. Guessing is safe here: this is diagnostics,
    /// and a wrong guess costs a misleading log line, not guest behaviour.
    fn render_arg(&self, v: u32) -> String {
        let bytes = self.mem.cstr_bytes(v);
        let printable = !bytes.is_empty()
            && bytes.len() < 128
            && bytes.iter().all(|&c| (0x20..0x7f).contains(&c));
        if printable {
            format!("{v:#x} \"{}\"", String::from_utf8_lossy(&bytes))
        } else {
            format!("{v:#x}")
        }
    }

    /// `name+offset` for a guest address, or bare hex if nothing owns it.
    ///
    /// The symbol table survived in this binary, which is the single largest
    /// thing working in this project's favour: nothing here has to speak in
    /// bare addresses.
    pub fn describe(&self, addr: u32) -> String {
        const REACH: u32 = 0x4000;
        let i = self.syms_by_addr.partition_point(|(a, _)| *a <= addr);
        if i == 0 {
            return format!("{addr:#010x}");
        }
        let (a, n) = &self.syms_by_addr[i - 1];
        if addr - a >= REACH {
            return format!("{addr:#010x}");
        }
        if *a == addr {
            n.clone()
        } else {
            format!("{n}+{:#x}", addr - a)
        }
    }

    /// Registers plus a best-effort backtrace, for fault reports.
    ///
    /// There are no frame pointers to walk, so the stack is scanned for words
    /// that point just past a `bl` inside the image. That over-reports — dead
    /// slots from earlier calls survive — but with symbol names attached it
    /// still identifies the path into a fault in seconds.
    pub fn context(&self, pc: u32) -> String {
        let mut s = String::new();
        for i in 0..13u8 {
            s.push_str(&format!("r{i}={:#010x} ", self.cpu.reg_get(Mode::User, i)));
            if i % 6 == 5 {
                s.push('\n');
            }
        }
        let sp = self.cpu.reg_get(Mode::User, reg::SP);
        let lr = self.cpu.reg_get(Mode::User, reg::LR);
        s.push_str(&format!(
            "\nsp={sp:#010x} lr={lr:#010x} ({})\npc={pc:#010x} ({})\n",
            self.describe(lr),
            self.describe(pc)
        ));

        s.push_str("probable callers (stack scan):\n");
        let lo = self.elf.base;
        let hi = self.elf.image_end;
        let mut shown = 0;
        let mut a = sp;
        while a < STACK_BASE + STACK_SIZE && shown < 16 {
            let w = self.mem.read_u32(a);
            // A return address points just past a `bl`, so the word four bytes
            // back must decode as one: cond != 0b1111, and bits 27..24 == 0b1011.
            if w > lo + 4 && w < hi {
                let prev = self.mem.read_u32(w - 4);
                if (prev >> 28) != 0xf && (prev >> 24) & 0xf == 0xb {
                    s.push_str(&format!("    {:#010x}  {}\n", w, self.describe(w)));
                    shown += 1;
                }
            }
            a += 4;
        }
        s
    }

    /// The `n` hottest guest functions from the profile, as
    /// `(name, samples, percent)`.
    pub fn profile_report(&self, top: usize) -> Vec<(String, u64, f64)> {
        let Some(p) = self.profile.as_ref() else { return Vec::new() };
        let total: u64 = p.values().sum();
        if total == 0 {
            return Vec::new();
        }
        let mut by_fn: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for (&addr, &n) in p {
            let name = self.describe(addr);
            let base = name.split('+').next().unwrap_or(&name).to_string();
            *by_fn.entry(base).or_insert(0) += n;
        }
        let mut v: Vec<(String, u64, f64)> = by_fn
            .into_iter()
            .map(|(n, c)| (n, c, c as f64 * 100.0 / total as f64))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(top);
        v
    }

    /// The hottest individual addresses *inside* one function, as
    /// `name+offset`.
    ///
    /// [`Vm::profile_report`] answers "which function"; once that function is a
    /// 7.9 KB blitter holding a dozen specialised inner loops, the next
    /// question is "which loop", and only the raw samples can answer it.
    /// Buckets are `granularity` bytes wide so a loop body shows up as one
    /// line instead of forty.
    pub fn profile_inside(&self, func: &str, granularity: u32, top: usize)
        -> Vec<(String, u64, f64)>
    {
        let Some(p) = self.profile.as_ref() else { return Vec::new() };
        let Some(start) = self.symbol(func) else { return Vec::new() };
        let end = start + self.elf.sizes.get(func).copied().unwrap_or(0x10000);
        let mut buckets: std::collections::HashMap<u32, u64> = Default::default();
        let mut inside = 0u64;
        for (&addr, &n) in p {
            if (start..end).contains(&addr) {
                inside += n;
                *buckets.entry((addr - start) / granularity.max(1)).or_insert(0) += n;
            }
        }
        if inside == 0 {
            return Vec::new();
        }
        let mut v: Vec<(String, u64, f64)> = buckets
            .into_iter()
            .map(|(b, c)| {
                let off = b * granularity.max(1);
                (format!("+{off:#06x}"), c, c as f64 * 100.0 / inside as f64)
            })
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(top);
        v
    }

    // ---- lifecycle ---------------------------------------------------------

    /// Run the C++ static constructors.
    ///
    /// `.init_array` holds one entry, `_GLOBAL__I_._code_Heroslore5.cpp`, which
    /// runs `__static_initialization_and_destruction_0(1, 65535)`. It must
    /// happen before any guest entry point: the sibling chronosSword had no
    /// `.init_array` at all, so this is the one loader-visible way in which this
    /// guest is harder than that one.
    ///
    /// The array's contents are `R_ARM_RELATIVE` targets, so they are only
    /// correct after the loader has run — which is why the addresses are read
    /// here rather than at parse time.
    pub fn run_init_array(&mut self) -> Result<(), String> {
        let (addr, count) = self.elf.init_array;
        for i in 0..count {
            let f = self.mem.read_u32(addr + i * 4);
            if f == 0 {
                continue;
            }
            let what = self.describe(f);
            self.call(f, &[]).map_err(|e| format!("in .init_array[{i}] ({what}): {e}"))?;
        }
        self.phase = Phase::CtorsRun;
        Ok(())
    }

    /// Run the C++ static destructors. The mirror of [`Vm::run_init_array`].
    pub fn run_fini_array(&mut self) -> Result<(), String> {
        let (addr, count) = self.elf.fini_array;
        for i in (0..count).rev() {
            let f = self.mem.read_u32(addr + i * 4);
            if f == 0 {
                continue;
            }
            let what = self.describe(f);
            self.call(f, &[]).map_err(|e| format!("in .fini_array[{i}] ({what}): {e}"))?;
        }
        Ok(())
    }

    /// Call a `Java_*` entry point with a synthetic `(JNIEnv*, jobject)` and
    /// any extra `int`-width arguments.
    pub fn call_native(&mut self, sym: &str, extra: &[u32]) -> Result<u32, String> {
        let addr = self.symbol(sym).ok_or_else(|| format!("no such JNI entry: {sym}"))?;
        let mut args = vec![self.jni_env(), 0];
        args.extend_from_slice(extra);
        self.call(addr, &args).map_err(|e| format!("in {sym}: {e}"))
    }

    /// Reproduce `CletEntry.onSurfaceCreated`'s boot sequence.
    ///
    /// The order and arguments were read from `classes.dex`
    /// (`MidasGLSurfaceView.<init>` sets the display fields, `onSurfaceCreated`
    /// makes the calls): InitKernel, then InitDisplay with the 16 display
    /// parameters, then file storage, then the VFS handle, then the audio
    /// buffer size (a literal `202400`), then StartApp. `nativeInitVFS` takes a
    /// Java `FileDescriptor`; headless, the VFS is already served by name from
    /// memory (see `files.rs`), so this passes a null descriptor and the guest
    /// falls back to opening `data.vfs.mp3` through the file layer.
    pub fn boot(&mut self) -> Result<(), String> {
        // The upright arrangement rewrites a few guest instructions and one
        // layout table in memory — before anything runs, and before any hook
        // is placed, so no hook ever sees half of it. Every original word is
        // checked first and a mismatch applies none of them. See `portrait`.
        if self.cfg.screen_mode.is_portrait() {
            let words = portrait::apply(self)?;
            self.log.push(format!(
                "portrait: {}x{} canvas, {} guest word(s) relaid for the keypad layout",
                self.cfg.wipi_w, self.cfg.wipi_h, words
            ));
        }
        // 공용창고 at ten pages: the warehouse object's layout, one immediate
        // per instruction, every word checked before any is written. Before
        // the first guest instruction for the same reason as the portrait
        // words: no allocation may happen at the old size. See `tenpages`.
        if self.cfg.ten_pages {
            let words = tenpages::apply(self, tenpages::STORAGE, "ten pages (warehouse)")?;
            // The 160-slot listing outgrows the 8 KB the guest copies every
            // inbound frame into; this is the word that sizes that copy.
            tenpages::apply(self, tenpages::RECV, "ten pages (receive buffer)")?;
            let bag_words = tenpages::apply(self, tenpages::BAG, "ten pages (bag)")?;
            let list_words = tenlists::apply(self, tenlists::WIDEN, "ten pages (lists)")?;
            let grid_words = tenpages::apply(self, tenpages::GRID, "ten pages (grid)")?;
            let mut names = tenpages::install_hooks(self)?;
            names.extend(tenpages::install_bag_hooks(self)?);
            self.log.push(format!(
                "ten pages: warehouse {} slots ({words} word(s)), frames up to {:#x} B received, \
                 bag {} slots ({bag_words} word(s)), lists widened past 127 ({list_words} word(s)), \
                 grid {} px right for two chest columns ({grid_words} word(s)); hooks on {}",
                tenpages::STORAGE_SLOTS,
                tenpages::recv_capacity(self),
                tenpages::BAG_PAGES_SIZE,
                tenpages::GRID_SHIFT,
                names.join(", ")
            ));
        }
        // Behaviour hooks, before the guest runs a single instruction. Two
        // features want the same `BFont::DrawText(iiiPcii)` entry — the
        // free-shop price line and the widened-canvas top bar — so the
        // dispatcher below owns that address and offers it to each in turn.
        // Each declines unless the return address is its own call site, which
        // is what makes the order between them irrelevant.
        if self.cfg.free_shop {
            let names = shop::install_hooks(self)?;
            self.log.push(format!("free-shop: hooks on {}", names.join(", ")));
        }
        // The 거래용 화폐 환전 control, removed while the warehouse is the
        // in-process one: it can only take money and never give it back, and
        // what the money buys (거래소) is shut offline. See
        // `warehouse::install_hooks`.
        if self.cfg.offline_menus {
            let names = warehouse::install_hooks(self)?;
            self.log.push(format!("warehouse: 환전 hidden; hooks on {}", names.join(", ")));
        }
        // Releasing OK or a skill no longer stops the walk, and ending an
        // attack / skill / dash at an EndProc site while the joystick is
        // still held walks again (facing 0..3, angle restored) instead of
        // standing. Two addresses, no shared dispatcher. See input.rs / §48.
        if self.cfg.keep_walking {
            let names = input::install(self)?;
            self.log.push(format!(
                "keep-walking: a non-direction key release no longer clears the move flags, \
                 and SetStandMotion walks when the stick is held; hook on {}",
                names.join(", ")
            ));
        }
        // Extra band-5 rolls of drop group 63 ((진)) and groups 64.. (every
        // set) on the hell lv97 bosses. The hook runs the original
        // `SetDropItem` first; each walk declines unless its table is long enough.
        // `MakeSocket` (from `NewDropItem` only) then fills a set piece's
        // reserved option slot the way `NewItemEx` does — that path never
        // called `MakeItemOption`, so the third line stayed `+랜덤옵션`.
        // `KeyStorage+0xa34` then lets a `(진)` past the guest's
        // `index > 0x50` warehouse fence; 공허 81..85 stay refused.
        {
            let names = legendaries::install_hooks(self)?;
            self.log.push(format!(
                "legendaries: endgame-weapon (row {}) and set-armour extra drop on \
                 hell groups 59..62 — same rate on both lines; set drops roll the \
                 reserved option; (진) warehouse deposit; hook on {}",
                if self.cfg.vanilla { legendaries::SOURCE_INDEX as u8 } else { legendaries::JIN_INDEX },
                names.join(", ")
            ));
        }
        // 정체모를 괴수 kills the looting window by starting a story scene, so
        // its drop is taken at death and routed through the guest's own
        // auto-pickup branch instead of onto the floor. The auto-pickup hook
        // re-reads the two words the original returns whenever this is not
        // holding its window open, so nothing else in the game moves.
        {
            let names = bossdrop::install_hooks(self)?;
            self.log.push(format!(
                "bossdrop: 정체모를 괴수 drops into the bag at death, plus {}% 장신구; hook on {}",
                bossdrop::ACCESSORY_PERCENT,
                names.join(", ")
            ));
        }
        // The added cash items: one dispatcher on the guest's use jump table,
        // and one multiplier on the monster-experience award. Both decline for
        // everything they do not own, so with an empty table neither changes a
        // single frame. See `cashitems` and `exp`.
        if self.cfg.new_cash_items && !self.cash.is_empty() {
            let names = cashitems::install_hooks(self)?;
            self.log.push(format!(
                "cashitems: {} new item(s); use-dispatcher hook on {}",
                self.cash.len(),
                names.join(", ")
            ));
            let names = exp::install(self)?;
            self.log.push(format!(
                "exp: 슈퍼 성장의 서 gives 3.0x (host 2x on top of the game's own +50%); hook on {}",
                names.join(", ")
            ));
        }
        // Buff durations and buff cooldowns armed in ticks converted from the
        // tenths of a second the skill text prints, so "34초" lasts 34 s, and
        // the buff rebalance's doubled durations (at the arm sites and in the
        // two texts). All hooks decline. See `skilltime`.
        if self.cfg.skill_time_hz > 0 || self.cfg.buff_rebalance {
            let names = skilltime::install(self)?;
            self.log.push(format!(
                "skill-time: buff timers {} ticks/s, rebalance {}; hooks on {}",
                self.cfg.skill_time_hz,
                if self.cfg.buff_rebalance { "on" } else { "off" },
                names.join(", ")
            ));
        }
        // Gamepad: one observer hook that reads the field HUD's own button
        // rects and declines. The buttons need nothing — this guest's touch UI
        // is itself a key synthesiser — and the stick goes in as a synthetic
        // finger on the guest's joystick, because movement in this build comes
        // from the joystick angle and not from direction keys. See `gamepad`.
        if self.cfg.gamepad {
            for line in padmap::install(self) {
                self.log.push(line);
            }
            let names = gamepad::install(self)?;
            self.log.push(format!(
                "gamepad: buttons -> guest keys, stick -> the guest's own joystick \
                 (pointer {}), badges {}; observer hook on {}",
                gamepad::PAD_POINTER,
                self.pad.hud.as_token(),
                names.join(", ")
            ));
        }
        // The `*아바타` button, the wardrobe, and the substitution at the one
        // place the hero's sprite sheets are named. See `avatar`.
        if self.cfg.avatar {
            let names = avatar::install(self)?;
            self.log.push(format!("avatar: wardrobe on the character page; hooks on {}", names.join(", ")));
            // 렌's wardrobe on the 정령 page, and the one place her sheets
            // are named. See `spirit_avatar`.
            let names = spirit_avatar::install(self)?;
            self.log.push(format!("avatar: 렌's wardrobe on the 정령 page; hooks on {}", names.join(", ")));
            // Scenes rebuild both as NPC bodies through `Map::NpcPathSet`,
            // which never passes `SetLoadImgPath`. See `npcbody`.
            let names = npcbody::install(self)?;
            self.log.push(format!("avatar: scene (NPC-body) copies of the hero and 렌 dressed too; hook on {}", names.join(", ")));
        }
        let hud_names = hud::install(self)?;
        if !hud_names.is_empty() {
            self.log.push(format!(
                "hud: canvas {} wide, in-game bar shifted +{} px",
                self.cfg.wipi_w, self.hud.dx
            ));
        }
        // The 해상도 row under the 환경설정 panel. Two hooks of its own, on a
        // screen nothing else touches. See `screen`.
        if self.cfg.screen_option {
            let names = options::install(self)?;
            self.log.push(format!(
                "options: 환경설정 {} rows (화면필터 {}, 패드키 {}, 해상도 {} ({})); hooks on {}",
                options::ROWS,
                if self.scale2x { "ON" } else { "OFF" },
                if self.padmap.custom { "커스텀" } else { "기본" },
                self.cfg.screen_mode.letter(),
                self.cfg.screen_mode.label(),
                names.join(", ")
            ));
            // The same two rows (no 해상도) on the in-game menu's 시스템 >
            // 환경설정, and the mapping screen over that menu. See `igoptions`.
            let names = igoptions::install(self)?;
            self.log.push(format!(
                "options: in-game 시스템 환경설정 {} rows; hooks on {}",
                igoptions::ROWS,
                names.join(", ")
            ));
            // The three save rows at the bottom of both lists: the shell's
            // document screens answer at the asking menu's next `Process`.
            // See `savexfer` and `savebundle`.
            let names = savexfer::install(self)?;
            self.log.push(format!(
                "savexfer: 세이브 내보내기/불러오기/되돌리기 ({} vault generation(s)); hooks on {}",
                savebundle::vault_list(&self.cfg.storage).len(),
                names.join(", ")
            ));
            // The 배치변경 row's layout: the field HUD drawn again from the
            // game's own pixels where the player put it. See `hudlayout`.
            let names = hudlayout::install(self)?;
            self.log.push(if names.is_empty() {
                "배치변경: not on this canvas (세로 or narrower than 320) — the row reads -".to_string()
            } else {
                format!(
                    "배치변경: {}; hooks on {}",
                    if self.hudlayout.custom { "커스텀" } else { "기본" },
                    names.join(", ")
                )
            });
        }
        if self.cfg.ten_pages || self.hudlayout.installed {
            let addr = self
                .symbol(DRAW_IMAGE_BLENDING)
                .ok_or_else(|| format!("`{DRAW_IMAGE_BLENDING}` is not in this binary"))?;
            self.install_hook(DRAW_IMAGE_BLENDING, addr, draw_image_blending)?;
        }
        if self.cfg.offline_menus || self.cfg.ten_pages {
            let addr = self
                .symbol(DRAW_IMAGE)
                .ok_or_else(|| format!("`{DRAW_IMAGE}` is not in this binary"))?;
            self.install_hook(DRAW_IMAGE, addr, draw_image)?;
        }
        if self.cfg.free_shop || !hud_names.is_empty() {
            let addr = self
                .symbol(DRAW_TEXT_PLAIN)
                .ok_or_else(|| format!("`{DRAW_TEXT_PLAIN}` is not in this binary"))?;
            self.install_hook(DRAW_TEXT_PLAIN, addr, draw_text_plain)?;
        }
        // Not a hook and not a patch — see `GUEST_REPAINT`. Installed before
        // the first guest instruction so the count covers the whole run, and
        // *not* fatal when absent: it is a measurement, and a binary without
        // the symbol should still boot. `Hud` treats "no counter" as "every
        // frame painted", which is the old behaviour.
        if !self.count_symbol(GUEST_REPAINT) {
            self.log.push(format!("no `{GUEST_REPAINT}` in this binary; repaints will read 0"));
        }
        self.rebuild_dispatch_filter();
        if self.cfg.free_shop {
            // Rows the derivation refuses, with the reason. A refused row
            // completes its purchase and grants nothing — which is the
            // deployed server's behaviour, and much better than the kind-18
            // listing token it used to hand over, which no screen in the game
            // accepts and which `KeyBagMenu` gives no menu at all
            // (ANALYSIS.md §32.10).
            for (id, why) in &self.shop.grants.rejected {
                self.log.push(format!("free-shop: catalogue row {id} grants nothing — {why}"));
            }
            // …and what the other rows *do* grant. Only the refusals used to be
            // reported, and that asymmetry is how a granted accessory no class
            // could wear stayed invisible until the equip gate was read
            // (ANALYSIS.md §39).
            for line in self.shop.grants.summary() {
                self.log.push(format!("free-shop: {line}"));
            }
            if self.shop.grants.is_empty() {
                // Purchases will still complete — with zero records, like the
                // deployed server. Saying so up front beats debugging an
                // empty bag later.
                self.log.push(
                    "free-shop: no shop_grants configured; purchases grant nothing \
                     (CLAUDE.md 캐시샵을 로컬로)"
                        .into(),
                );
            }
        }
        self.run_init_array()?;
        self.call_native("Java_co_kr_eamobile_CletEntry_nativeInitKernel", &[])?;

        // nativeInitDisplay(screenW, screenH, r, g, b, a, wipiW, wipiH,
        //                   marginX, marginY, colorType, magnification(f32),
        //                   magFilter, viewMode, useSublayer, sublayerPixel)
        //
        // The scale and margins are not ours to invent — `adjustAutoScreen`
        // computes them and the guest places its own quad from them, so
        // getting them wrong draws a 400x240 picture into the corner of a
        // phone-sized framebuffer.
        self.fit_screen();
        self.call_native(
            "Java_co_kr_eamobile_CletEntry_nativeInitDisplay",
            &[
                self.cfg.screen_w as u32,
                self.cfg.screen_h as u32,
                5, // red bits   (RGB565 — matches the whole 565 blitter family)
                6, // green bits
                5, // blue bits
                0, // alpha bits
                self.cfg.wipi_w as u32,
                self.cfg.wipi_h as u32,
                self.margin_x as u32,
                self.margin_y as u32,
                0, // wipiColorType (0 = default/565)
                self.magnification.to_bits(),
                0, // targetMagFilterType
                0, // targetViewMode
                0, // bUseSublayer
                0, // sublayerPixelType
            ],
        )?;

        let internal = self.cstr_to_guest(INTERNAL_STORAGE_ROOT);
        let external = self.cstr_to_guest("/sdcard/eamobile/HeroesLore5");
        self.call_native(
            "Java_co_kr_eamobile_CletEntry_nativeInitFileStorage",
            &[internal, external],
        )?;

        // nativeInitVFS(FileDescriptor, startOffset(long), length(long)).
        // The guest reads its VFS through the fd, not by name: it pulls the
        // `descriptor` int out of the FileDescriptor object, dups it, and
        // fdopens the copy. So hand it a real VFS fd wrapped in an object.
        //
        // The argument layout is not the naive one. AAPCS gives a `jlong` an
        // even-numbered register pair, and after (JNIEnv* r0, jclass r1,
        // jobject r2) the next free register is the odd r3 — so **r3 is
        // padding** and both longs land on the stack:
        //
        //     r0 env   r1 clazz   r2 FileDescriptor   r3 (pad)
        //     [sp+0x20] startOffset.lo   [sp+0x24] .hi
        //     [sp+0x28] length.lo        [sp+0x2c] .hi
        //
        // which is exactly what the entry point reads (`ldr sl, [sp, #0x20]`,
        // `ldr sb, [sp, #0x28]` at nativeInitVFS+0x1c/+0x20, handed to
        // `setVFSInfo(fd, start, length)`). Dropping the pad word shifts every
        // stack argument down one and `length` arrives as 0 — the guest then
        // walks exactly one archive entry before `cursor < length` fails, and
        // every VFS lookup returns -1.
        let vfs_fd = self.open_vfs_fd();
        let fd_obj = self.new_int_object("java/io/FileDescriptor", vfs_fd as i32);
        let vfs_len = self.cfg.vfs.len() as u64;
        self.call_native(
            "Java_co_kr_eamobile_CletEntry_nativeInitVFS",
            &[
                fd_obj,
                0,                       // r3 — jlong alignment padding
                0,                       // startOffset.lo — the archive is the
                0,                       // startOffset.hi   whole served file
                vfs_len as u32,          // length.lo
                (vfs_len >> 32) as u32,  // length.hi
            ],
        )?;

        // nativeInitAudioConfiguration(long bufferSize) — literal 202400.
        self.call_native(
            "Java_co_kr_eamobile_CletEntry_nativeInitAudioConfiguration",
            &[202400, 0],
        )?;

        self.call_native("Java_co_kr_eamobile_CletEntry_nativeStartApp", &[])?;
        self.phase = Phase::Running;

        // `onStartApp` has run `MX_desInit`, so the key schedule exists: prove
        // the host DES against the guest's own now, not at the save button.
        // A no-op without the `startDes` port. See `des`.
        self.des_selftest();

        if self.cfg.boot_mode == BootMode::SkipNetwork {
            self.skip_pending = true;
            self.try_skip_network()?;
        }
        Ok(())
    }

    /// [`BootMode::SkipNetwork`]'s jump: move `StateMainMenu` from state 0/1
    /// straight to state 2 (Logo), **with guest calls only** — the same calls,
    /// in the same order, that the guest's own popup-0x38/0x39 result handler
    /// (`StateMainMenu::Process`, 0x124E24) makes once the network attempt has
    /// failed or succeeded. The handler's first call, `SetPopupState(0)`, is
    /// deliberately absent: it clears the popup the network attempt created,
    /// and this runs before any frame, when no popup has ever existed. States
    /// 0/1 then never run, so `endProcess` never reaches `netConnect` and
    /// neither "Connecting" nor the error popups exist. The intro that follows
    /// is untouched.
    ///
    /// The acceptance test for this is the aligned A/B of ANALYSIS.md §16.10:
    /// film both boot modes with `--film-every 1`, align on the first Logo
    /// frame, and every frame after the seam must be byte-identical — which is
    /// only provable because nothing here writes guest memory. The one read
    /// (`gv+0x27c`, the state object's pointer) decides *whether* to call, not
    /// what the guest computes.
    ///
    /// A missing symbol is a hard error: this jump is keyed to names, exactly
    /// like `patch.rs`, so a wrong binary is loud rather than subtly other.
    fn try_skip_network(&mut self) -> Result<(), String> {
        let need = |vm: &Vm, name: &str| -> Result<u32, String> {
            vm.symbol(name)
                .ok_or_else(|| format!("skip-network: `{name}` is not in this binary"))
        };
        let gv = need(self, "gv")?;
        let get_state = need(self, "_ZN13StateMainMenu8GetStateEv")?;
        let load_logo = need(self, "_ZN13StateMainMenu14LoadLogoSpriteEa")?;
        let set_state = need(self, "_ZN13StateMainMenu8SetStateEa")?;

        let st = self.mem.read_u32(gv + GV_STATE_MAIN_MENU);
        if st == 0 {
            // Not constructed yet. Retry from `frame`, bounded.
            self.skip_tries += 1;
            if self.skip_tries > SKIP_GIVE_UP_FRAMES {
                self.skip_pending = false;
                self.log.push(format!(
                    "skip-network: StateMainMenu never appeared at gv+{GV_STATE_MAIN_MENU:#x} \
                     within {SKIP_GIVE_UP_FRAMES} frames; running the original boot"
                ));
            }
            return Ok(());
        }

        // `GetState` returns a sign-extended char; the states are 0..=12.
        let state = self.call(get_state, &[st])? & 0xff;
        match state {
            0 | 1 => {
                self.call(load_logo, &[st, 0])?;
                self.call(set_state, &[st, 2])?;
                self.skip_pending = false;
                self.log.push(format!(
                    "skip-network: StateMainMenu {state} -> 2 (Logo); states 0/1 never ran"
                ));
            }
            _ => {
                // Already past the network states — the guest got there on its
                // own (or a replay drove it); there is nothing left to skip.
                self.skip_pending = false;
            }
        }
        Ok(())
    }

    /// Install the host reimplementations in [`patch::PATCHES`].
    ///
    /// Returns the names that were installed. A name the binary does not have
    /// is an error, not a silent skip: patches are matched by symbol precisely
    /// so that a stale one is loud.
    pub fn install_patches(&mut self) -> Result<Vec<&'static str>, String> {
        let mut on = Vec::new();
        for (name, f) in patch::PATCHES {
            let addr = self
                .symbol(name)
                .ok_or_else(|| format!("patch target `{name}` is not in this binary"))?;
            if addr < self.elf.base || addr % 4 != 0 {
                // The bitmap below indexes ARM words above the image base;
                // an entry point that is neither is not a function this
                // binary can contain (ARM-only, no Thumb), so refuse loudly.
                return Err(format!("patch target `{name}` at {addr:#010x} is not an ARM word in the image"));
            }
            self.patches.insert(addr, *f);
            on.push(*name);
        }
        // The font-pixel port reads thirteen `_midas_*` globals and runs once
        // per glyph pixel — millions of times a replay — so their addresses
        // are resolved here, once, rather than by name per call.
        self.fnt = patch::FntGlobals::resolve(self);
        if self.fnt.is_none() {
            self.log.push(
                "patch: the font-pixel globals did not all resolve;                  `_midas_funcFntDrawPixel` will decline"
                    .into(),
            );
        }
        self.rebuild_dispatch_filter();
        Ok(on)
    }

    /// Put a behaviour hook on a guest symbol, with the checks a patch gets.
    ///
    /// Separate from [`Vm::patches`] on purpose: that map's contract is
    /// "produces the same bytes", proven by the offline 905-frame A/B, and a
    /// hook deliberately changes what happens. Same dispatch, same prefilter
    /// bitmap, different meaning — see [`Vm::hooks`].
    ///
    /// The caller must [`Vm::rebuild_dispatch_filter`] once it has installed
    /// everything it means to.
    pub(crate) fn install_hook(
        &mut self,
        name: &str,
        addr: u32,
        f: patch::PatchFn,
    ) -> Result<(), String> {
        // The same guard `install_patches` applies: the dispatch bitmap
        // indexes ARM words above the image base, so an entry point that is
        // neither would underflow into a bit nothing ever sets and the hook
        // would silently not exist.
        if addr < self.elf.base || addr % 4 != 0 {
            return Err(format!(
                "hook target `{name}` at {addr:#010x} is not an ARM word in the image"
            ));
        }
        if self.hooks.insert(addr, f).is_some() {
            return Err(format!(
                "two hooks on `{name}` ({addr:#010x}); they must share a dispatcher"
            ));
        }
        Ok(())
    }

    /// Rebuild the per-word bitmap [`Vm::run`] prefilters with, sized to the
    /// highest patched or hooked address rather than the whole image.
    ///
    /// Must be called after **any** of the three maps changes.
    /// `install_patches` runs before `boot`, and `boot` installs the free-shop
    /// hooks, so a filter built only from `patches` would leave every hook
    /// unreachable — the bit would never be set and the map never consulted.
    /// [`Vm::count_symbol`] rebuilds it for itself.
    pub(crate) fn rebuild_dispatch_filter(&mut self) {
        let words: Vec<u32> = self
            .patches
            .keys()
            .chain(self.hooks.keys())
            .chain(self.probes.keys())
            .map(|a| (a - self.elf.base) >> 2)
            .collect();
        if words.is_empty() {
            self.dispatch_filter.clear();
            return;
        }
        let top = words.iter().copied().max().unwrap_or(0);
        self.dispatch_filter = vec![0u64; (top >> 6) as usize + 1];
        for w in words {
            self.dispatch_filter[(w >> 6) as usize] |= 1u64 << (w & 63);
        }
    }

    /// Call a guest `m_pfnPixelOp` from inside a patched blitter.
    ///
    /// Re-entering the guest from host code is the same mechanism `qsort`'s
    /// comparator uses, and it is why a ported blitter cannot simply assume
    /// the pixel op is absent: when the context has one, the shipped code
    /// calls it for every pixel and the output depends on it.
    pub(crate) fn call_pixel_op(&mut self, pfn: u32, dst: u32, col: u32, param: u32) -> u32 {
        self.call(pfn, &[dst, col, param]).unwrap_or(col)
    }

    /// Reproduce `CletEntry.adjustAutoScreen`: the largest uniform scale that
    /// fits the 400x240 WIPI screen inside the display, centred, with the
    /// remainder as margins.
    ///
    /// The dex computes it on the display's **long** and **short** sides
    /// rather than width and height — the game is landscape, and the original
    /// read the display before the rotation settled. Reproducing that exactly
    /// matters because the guest keeps these three numbers: it places its own
    /// textured quad with them, and `onTouchHandleEvent` divides incoming
    /// touches by them.
    ///
    /// "Long and short" assumes a landscape canvas. The portrait one is 240x320
    /// and is drawn into an upright window, so the display is read the same way
    /// round as the canvas instead: swapping it would hand the guest a 0.75
    /// magnification and every touch would land a third too far out.
    fn fit_screen(&mut self) {
        let (mag, mx, my) =
            fit_screen(self.cfg.screen_w, self.cfg.screen_h, self.cfg.wipi_w, self.cfg.wipi_h);
        self.magnification = mag;
        self.margin_x = mx;
        self.margin_y = my;
    }

    /// Where a touch in display pixels lands in the guest's WIPI screen.
    ///
    /// `CletEntry.onTouchHandleEvent` does exactly this before calling
    /// `nativeTouchHandleEvent`: subtract the margin, divide by the
    /// magnification, clamp to the WIPI screen. A host that skips it hands the
    /// guest display coordinates and every tap lands somewhere else.
    pub fn map_touch(&self, x: f64, y: f64) -> (i32, i32) {
        let mag = self.magnification.max(0.001) as f64;
        let gx = ((x - self.margin_x as f64) / mag) as i32;
        let gy = ((y - self.margin_y as f64) / mag) as i32;
        (gx.clamp(0, self.cfg.wipi_w - 1), gy.clamp(0, self.cfg.wipi_h - 1))
    }

    /// The 화면필터 row moved: the new value, once. The shell applies it to
    /// its presenter; the probe has nothing to apply it to.
    pub fn take_scale2x_change(&mut self) -> Option<bool> {
        std::mem::take(&mut self.scale2x_dirty).then_some(self.scale2x)
    }

    /// A save row wants the phone's document screen — once. The shell opens
    /// it and reports back with [`Vm::save_doc_done`]; the probe stands in
    /// with `--save-doc`.
    pub fn take_save_doc_request(&mut self) -> Option<savexfer::DocRequest> {
        self.savexfer.request.take()
    }

    /// How the document screen ended. Acted on at the asking menu's next
    /// `Process`, so this may be called whenever the shell hears of it.
    pub fn save_doc_done(&mut self, status: savexfer::DocStatus) {
        savexfer::done(self, status);
    }

    /// The host framebuffer the GL capture layer blits into: `0x00RRGGBB`
    /// pixels, row-major, `w * h` of them.
    pub fn framebuffer(&self) -> (&[u32], i32, i32) {
        (&self.gl.fb, self.gl.fb_w, self.gl.fb_h)
    }

    /// `CletEntry.surfaceDestroyed` → `nativePauseApp`. The activity going to
    /// the background; the guest stops its timers and saves.
    pub fn pause(&mut self) -> Result<(), String> {
        self.call_native("Java_co_kr_eamobile_CletEntry_nativePauseApp", &[])?;
        Ok(())
    }

    /// The counterpart of [`Vm::pause`]. `onResume` in the original shell.
    pub fn resume(&mut self) -> Result<(), String> {
        self.call_native("Java_co_kr_eamobile_CletEntry_nativeResumeApp", &[])?;
        Ok(())
    }

    /// Run one `nativeLoop` — one guest frame. Advances the virtual clock
    /// first so the guest's WIPI timers fire and it actually draws.
    pub fn frame(&mut self, dt_ms: i64) -> Result<(), String> {
        // Tell the bar's tap translation whether the frame that just finished
        // actually put a picture on the screen.
        //
        // "Once per `Vm::frame`" is the wrong clock for that question and
        // used to be the one asked: most polls run the timer list, find
        // nothing due, and draw nothing, so a span cleared every frame is
        // absent for two polls out of three and the in-game bar becomes hard
        // to press (ANALYSIS.md §31.9). What the translation actually needs is
        // "is the bar in the picture that is on screen", and that changes only
        // when the guest repaints. Zero here — no counter, or the guest has
        // not repainted yet — reads as "did not paint", which keeps the span.
        let repaints = self.guest_repaints();
        let painted = repaints != self.repaints_seen;
        self.repaints_seen = repaints;
        self.hud.frame_start(painted);
        self.hudlayout.frame_start(painted);
        // The pad rides the same clock for the same reason, and then hands
        // over whatever the guest's one-slot input mailboxes will take. Before
        // `nativeLoop`, so an edge that arrived while the host was asleep is
        // in the mailbox when this tick drains it rather than the next one.
        self.pad_frame_start(painted);
        self.pad_pump()?;
        if self.skip_pending {
            self.try_skip_network()?;
        }
        self.advance_time(dt_ms);
        self.deliver_mtx_result()?;
        self.call_native("Java_co_kr_eamobile_CletEntry_nativeLoop", &[])?;
        Ok(())
    }

    /// Report a completed carrier payment, if one is outstanding.
    ///
    /// On a device this is the activity calling back into the library after
    /// the carrier's payment UI closes, so it happens *after* `requestMTX`
    /// returns, not inside it. Firing it from the next frame keeps that
    /// ordering, which matters: `nativeResultMTX` (0x1526EC) drops the guest's
    /// `bProgress` flag and calls `MTX_Result_CB` (0xCE3E0), and that callback
    /// is what emits `WriteData(6, 5)` — the packet the server grants the item
    /// on (docs/SERVER.md 6.5). Called from inside `requestMTX` it would run
    /// while the guest is still on the JNI side of its own purchase call.
    ///
    /// `nativeResultMTX(env, cls, jstring receipt, jint result, jint code)`:
    /// `MTX_Result_CB` treats `result == 0` as paid, `code == -10` as the user
    /// cancelling, and anything else as an error to show. There is no carrier
    /// here, so it is always paid, with no receipt string.
    fn deliver_mtx_result(&mut self) -> Result<(), String> {
        if !std::mem::take(&mut self.pending_mtx_result) {
            return Ok(());
        }
        self.call_native(
            "Java_co_kr_eamobile_mtx_CarrierFuncActivity_nativeResultMTX",
            &[0, 0, 0],
        )?;
        Ok(())
    }

    /// One key transition, exactly as `CletEntry.onKeyHandleEvent` delivers it.
    ///
    /// The dex (`tools_re/dexmethod.py payload/classes.dex CletEntry
    /// onKeyHandleEvent`) turns an Android `KeyEvent` into
    /// `nativeKeyHandleEvent(type, WIPIConstant.getWIPIKeyCode(code), 0)`,
    /// with type 2 for ACTION_DOWN and 3 for ACTION_UP. `code` here is already
    /// the WIPI code — see [`wipi_key`].
    pub fn key(&mut self, code: i32, pressed: bool) -> Result<(), String> {
        let ty = if pressed { EV_KEY_PRESSED } else { EV_KEY_RELEASED };
        self.call_native(
            "Java_co_kr_eamobile_CletEntry_nativeKeyHandleEvent",
            &[ty, code as u32, 0],
        )?;
        Ok(())
    }

    /// One touch transition in WIPI screen coordinates (400x240 origin), from
    /// the finger identified by `pointer`.
    ///
    /// `CletEntry.onTouchHandleEvent` maps the device pixel through
    /// `targetMargin*` / `targetMagnification` first and clamps to the WIPI
    /// screen, then calls `nativeTouchHandleEvent(type, x, y, pointerId)` with
    /// 19 pressed, 20 released, 21 moved. The probe is already in WIPI
    /// coordinates, so it starts after that mapping.
    ///
    /// # The fourth argument is not decoration
    ///
    /// It used to be a literal `0` here, and that quietly reproduced a branch
    /// the original never ran. `CletEntry.checkTouchType` is three dex
    /// instructions — `bDecidedTouchType = true; bSupportMultitouch = true;
    /// return` — so the shipped game took its **multitouch** path on every
    /// device and passed a real `getPointerId(i)` (ANALYSIS.md §37.4). The id
    /// survives the whole way in: `nativeTouchHandleEvent` (0x14E6A4) →
    /// `handleCletEvent` (0x43124) → `clsidAuthHandle` (0x42FD0, which does
    /// not touch r3) → `onHandleEvent` (0x9C20C, `mov r5, r3`) →
    /// `BaseState::Touch{Press,Release,Move}(x, y, id)` at `_ZTV9BaseState`+8
    /// slots +0x30 / +0x34 / +0x38.
    ///
    /// And the game uses it: the touch joystick has an **owning finger**.
    /// `StateGame::TouchPressed+0x250` stores the pressing id in `gv+0x124`,
    /// and `StateGame::TouchReleased+0xdc` (0xE18E8) releases the stick only
    /// when `cmp id, (i8)gv+0x124` matches. With every finger claiming id 0
    /// that test was always true, so lifting the OK / skill / dash button let
    /// go of the joystick.
    ///
    /// # Why it is clamped to 0..=0x7f
    ///
    /// The guest keeps the owner in a signed byte and compares it
    /// sign-extended (`lsl #0x18` / `asr #24` at 0xE18EC), so only −128..127
    /// round-trip, and **−99 is the "nobody owns it" sentinel**
    /// (`mvn r2, #0x62` at 0xE1944). Staying non-negative cannot collide with
    /// it. Callers hand over small slot indices for exactly this reason — see
    /// `App::touch_slot` in `hl5_android`.
    pub fn touch(&mut self, kind: Touch, x: i32, y: i32, pointer: i32) -> Result<(), String> {
        // The key-mapping screen owns real fingers while it is up; the pad's
        // own synthetic finger never reaches it (the pad does not steer then).
        if self.padmap.active
            && pointer != gamepad::PAD_POINTER
            && padmap::touch(self, kind, x, y)
        {
            return Ok(());
        }
        // …and so does the 배치변경 editor.
        if self.hudlayout.edit.active
            && pointer != gamepad::PAD_POINTER
            && hudedit::touch(self, kind, x, y, pointer)
        {
            return Ok(());
        }
        // A finger on a control the 배치변경 layout moved goes to where the
        // guest still believes it is; a finger on the place a control left
        // goes nowhere. The pad's synthetic finger already speaks guest
        // coordinates.
        let (x, y) = if pointer != gamepad::PAD_POINTER {
            match hudlayout::route(self, kind, x, y, pointer) {
                Some(p) => p,
                None => return Ok(()),
            }
        } else {
            (x, y)
        };
        let ty = match kind {
            Touch::Pressed => EV_TOUCH_PRESSED,
            Touch::Released => EV_TOUCH_RELEASED,
            Touch::Moved => EV_TOUCH_MOVED,
        };
        // A tap on the in-game top bar lands where the host moved the bar to,
        // not where the guest lays it out; put it back before the guest's own
        // hit test sees it. Everything else passes through untouched.
        let (x, y) = self.hud.unshift(x, y);
        let x = x.clamp(0, self.cfg.wipi_w - 1) as u32;
        let y = y.clamp(0, self.cfg.wipi_h - 1) as u32;
        let pointer = pointer.clamp(0, 0x7f) as u32;
        self.call_native(
            "Java_co_kr_eamobile_CletEntry_nativeTouchHandleEvent",
            &[ty, x, y, pointer],
        )?;
        Ok(())
    }

    /// Write a C string into the guest heap and return its address.
    fn cstr_to_guest(&mut self, s: &str) -> u32 {
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        let p = self.heap.alloc(b.len() as u32).unwrap_or(0);
        if p != 0 {
            self.mem.write_bytes(p, &b);
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_default_speed_is_the_old_expression() {
        for ms in [0u64, 1, 33, 1_000, 86_400_000, 4_000_000_000] {
            let d = Duration::from_millis(ms);
            assert_eq!(scale_elapsed(d, 1.0), d.as_millis() as i64, "at {ms} ms");
        }
    }

    #[test]
    fn scaling_is_monotonic_and_precise_over_a_long_session() {
        // An f32 coefficient quantises to ~0.25 ms after an hour; the guest
        // reads this monotonically, so a stutter backwards is a real bug.
        let speed = 1.5;
        let mut prev = i64::MIN;
        for ms in (0..7_200_000).step_by(37) {
            let now = scale_elapsed(Duration::from_millis(ms), speed);
            assert!(now >= prev, "went backwards at {ms} ms: {now} < {prev}");
            prev = now;
        }
        assert_eq!(scale_elapsed(Duration::from_millis(7_200_000), speed), 10_800_000);
    }

    #[test]
    fn an_absurd_speed_saturates_rather_than_wrapping() {
        let d = Duration::from_millis(1_000_000);
        assert_eq!(scale_elapsed(d, 1.0e300), i64::MAX);
        assert_eq!(scale_elapsed(d, 0.0), 0);
        assert_eq!(scale_elapsed(d, -1.0), 0, "a negative coefficient must not run time backwards");
    }

    #[test]
    fn the_canvas_scales_by_a_whole_number_on_real_displays() {
        // (surface, expected canvas, expected magnification)
        for (sw, sh, cw, ch, k) in [
            (2340, 1080, 585, 270, 4), // S25 Ultra
            (2220, 1080, 555, 270, 4), // S9
            (1920, 1080, 480, 270, 4),
            (1280, 720, 426, 240, 3),
            (960, 540, 480, 270, 2),
            (800, 480, 400, 240, 2),
        ] {
            assert_eq!(widened_canvas(sw, sh), (cw, ch), "for {sw}x{sh}");
            assert_eq!(sh / ch, k, "vertical magnification for {sw}x{sh}");
            assert_eq!(sh % ch, 0, "{sw}x{sh}: the height must divide exactly");
            assert!(cw * k <= sw, "{sw}x{sh}: the canvas must not be wider than the surface");
            assert!(sw - cw * k < k, "{sw}x{sh}: at most one device pixel of slack");
        }
    }

    /// The 해상도 setting narrows the canvas and **nothing else**. What has to
    /// stay true is the magnification: the height is untouched, so a canvas
    /// pixel is still exactly `k` device pixels across and the picture is
    /// letterboxed rather than resampled. See [`screen`].
    #[test]
    fn the_resolution_setting_narrows_only_the_width() {
        for (sw, sh) in [(2340, 1080), (2220, 1080), (1920, 1080), (1280, 720)] {
            let (full_w, full_h) = widened_canvas(sw, sh);
            let k = sh / full_h;
            for mode in ScreenMode::LANDSCAPE {
                let (w, h) = widened_canvas_for(sw, sh, mode);
                assert_eq!(h, full_h, "{sw}x{sh} {mode:?}: the height must not move");
                assert_eq!(sh / h, k, "{sw}x{sh} {mode:?}: the magnification must not move");
                assert!(w <= full_w, "{sw}x{sh} {mode:?}: narrower, never wider");
                assert!(w >= WIPI_W, "{sw}x{sh} {mode:?}: never below the authored width");
                if mode == ScreenMode::Full {
                    assert_eq!(w, full_w, "mode A must be byte-for-byte the old behaviour");
                } else {
                    // Within a pixel of the advertised percentage, which is
                    // what integer division costs.
                    let want = full_w * mode.width_pct().unwrap() / 100;
                    assert_eq!(w, want.max(WIPI_W), "{sw}x{sh} {mode:?}");
                }
            }
        }
        // The three widths the shipping phone gets, spelled out.
        assert_eq!(widened_canvas_for(2340, 1080, ScreenMode::Full), (585, 270));
        assert_eq!(widened_canvas_for(2340, 1080, ScreenMode::Narrow10), (526, 270));
        assert_eq!(widened_canvas_for(2340, 1080, ScreenMode::Narrow20), (468, 270));
    }

    /// 세로 is the guest's own 240x320 whatever the display, either way up —
    /// the window it lands in is a box the shell cuts to that shape.
    #[test]
    fn the_portrait_canvas_does_not_follow_the_display() {
        for (sw, sh) in [(1080, 2340), (2340, 1080), (1080, 2220), (720, 1280), (800, 480)] {
            assert_eq!(widened_canvas_for(sw, sh, ScreenMode::Portrait), (240, 320), "{sw}x{sh}");
        }
    }

    /// `fit_screen` reads the display the same way round as the canvas. The
    /// landscape answer is the old one exactly; the portrait one has to be
    /// 1.0 and no margins, because the shell already scaled the finger.
    #[test]
    fn the_guests_magnification_is_one_on_both_canvases() {
        assert_eq!(fit_screen(585, 270, 585, 270), (1.0, 0, 0));
        assert_eq!(fit_screen(240, 320, 240, 320), (1.0, 0, 0));
        // The landscape canvas still reads a display reported upright, which is
        // what the original's long/short rule was for.
        assert_eq!(fit_screen(270, 585, 585, 270), (1.0, 0, 0));
        // And the one the old rule got wrong: an upright canvas read sideways
        // would have been 0.75.
        assert_eq!(fit_screen(320, 240, 240, 320), (1.0, 0, 0));
        // Unchanged where it already worked: the original 400x240 on a S9.
        let (mag, mx, my) = fit_screen(2220, 1080, 400, 240);
        assert_eq!((mag, mx, my), (4.5, 210, 0));
    }

    /// A display small enough that 90% would crop the UI keeps the full canvas
    /// rather than losing the edges of it. There is nothing to give back.
    #[test]
    fn a_narrow_display_declines_the_setting_instead_of_cropping() {
        let (full_w, _) = widened_canvas(800, 480);
        assert_eq!(full_w, WIPI_W, "800x480 is already at the authored width");
        for mode in ScreenMode::LANDSCAPE {
            assert_eq!(widened_canvas_for(800, 480, mode).0, WIPI_W, "{mode:?}");
        }
    }

    #[test]
    fn every_plausible_display_gets_a_legal_canvas_and_fills_its_height() {
        // Mirrors `Vm::fit_screen`: the magnification is the *smaller* of the
        // two ratios, so nothing can ever overflow — the question a sweep can
        // answer is whether the picture still fills the screen, and whether the
        // integer rule ever hands back something the guest cannot draw into.
        for sh in (240..=2160).step_by(3) {
            for sw in [sh, sh * 4 / 3, sh * 16 / 9, sh * 195 / 90, sh * 21 / 9, sh * 4] {
                let (cw, ch) = widened_canvas(sw, sh);
                assert!(ch >= WIPI_H && cw >= WIPI_W && cw <= 1024, "{sw}x{sh} -> {cw}x{ch}");

                let mag = (sw as f64 / cw as f64).min(sh as f64 / ch as f64);
                let (fw, fh) = (cw as f64 * mag, ch as f64 * mag);
                assert!(fw <= sw as f64 + 1e-6, "{sw}x{sh} -> {cw}x{ch}: {fw} wide");
                assert!(fh <= sh as f64 + 1e-6, "{sw}x{sh} -> {cw}x{ch}: {fh} tall");

                // A landscape surface wide enough for the authored 400 must
                // fill its height — a horizontal band of black is the thing
                // §27 set out to remove and must not creep back. Under a whole
                // number magnification the remainder is at most one canvas
                // pixel, which is what `mag` measures.
                if sw >= sh && sw * WIPI_H >= sh * WIPI_W {
                    let letterbox = sh as f64 - fh;
                    assert!(letterbox < mag, "{sw}x{sh} -> {cw}x{ch}: {letterbox}px letterbox");
                }
            }
        }
    }

    #[test]
    fn the_integer_branch_leaves_no_slack_worth_seeing() {
        // Where the whole-number rule applies, both extents must come out exact
        // to within one magnification step; where it declines, the fallback
        // must still be the old design height.
        let (mut integral, mut fallback) = (0, 0);
        for sh in (240..=2160).step_by(3) {
            for sw in [sh * 4 / 3, sh * 16 / 9, sh * 195 / 90, sh * 21 / 9] {
                let (cw, ch) = widened_canvas(sw, sh);
                if ch != WIPI_H || sh / WIPI_H * WIPI_H == sh {
                    // `k` divides the surface height only when it happens to;
                    // what the rule guarantees is a whole-number magnification,
                    // so the slack on both axes is under one device step.
                    let k = sh / ch;
                    assert!(k >= 1 && ch * k <= sh, "{sw}x{sh} -> {cw}x{ch}: k={k}");
                    assert!(sh - ch * k < k, "{sw}x{sh} -> {cw}x{ch}: {}px tall slack", sh - ch * k);
                    assert!(sw - cw * k < k, "{sw}x{sh} -> {cw}x{ch}: {}px wide slack", sw - cw * k);
                    integral += 1;
                } else {
                    assert_eq!(ch, WIPI_H, "{sw}x{sh}: fallback must keep the design height");
                    fallback += 1;
                }
            }
        }
        assert!(integral > fallback, "{integral} integral vs {fallback} fallback");
    }

    #[test]
    fn a_portrait_or_unsettled_surface_still_gets_a_legal_canvas() {
        for (sw, sh) in [(1080, 2340), (0, 0), (1, 1), (240, 240), (100, 4000)] {
            let (cw, ch) = widened_canvas(sw, sh);
            assert!(cw >= WIPI_W && cw <= 1024 && ch >= WIPI_H, "{sw}x{sh} -> {cw}x{ch}");
        }
    }
}
