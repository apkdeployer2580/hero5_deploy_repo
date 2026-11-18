//! `hl5_android` — the aarch64 Android host for the heroesLore5 guest.
//!
//! Everything interesting is in `hl5_core`; this crate is the shell around it.
//! It pulls the two payload files out of the APK's assets, sizes the guest
//! screen from the real window, and pumps frames and touches. The whole
//! resulting library is AArch64 — nothing here is 32-bit, and the APK ships a
//! single `arm64-v8a` ABI, which is the entire point of the project.
//!
//! The VM runs on the winit thread. That is a deliberate simplification over
//! the worker-thread arrangement a JVM-hosted emulator needs: a frame is a
//! bounded amount of work that ends in exactly the blit we then present, so
//! there is nothing to synchronise.
//!
//! Sound goes out the way the original sent it. `hl5_core::audio` records the
//! byte range of every clip inside the archive and queues the guest's
//! play/stop/pause calls; [`sound`] drains that queue once a frame onto
//! `SoundPool` and `MediaPlayer`, which decode the Ogg Vorbis themselves. No
//! decoder or mixer of ours is involved, because none was involved in 2011
//! either.

#[cfg(target_os = "android")]
mod sound;
#[cfg(target_os = "android")]
mod savedoc;

mod egl;

mod affinity;

use hl5_core::{BootMode, Config, Touch, Vm};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// Files bundled in the APK under `assets/`.
pub const ASSET_GUEST: &str = "libHeroesLore5.so";
pub const ASSET_VFS: &str = "data.vfs.mp3";

/// How often a finished picture is offered to the compositor.
///
/// The original paced itself with its own WIPI timers and let
/// `CletEntry.onDrawFrame` run at the display rate; this is that rate. It is
/// **not** the rate the guest is polled at — see [`POLL`].
const FRAME: Duration = Duration::from_micros(16_667);

/// How often the guest is offered a chance to run.
///
/// This is a separate, faster grid than [`FRAME`], and the reason is that one
/// `nativeLoop` advances the guest by **at most one game tick**: `timerLoop`
/// (0x152318) walks the timer list once. So whatever this loop's period is,
/// it is also a hard ceiling on the guest's tick rate — and it quantises it,
/// because a tick due 36 ms from now can only be served on a poll boundary.
///
/// The guest's design period is 36 ms, not the 45 ms its `MC_knlSetTimer`
/// argument suggests: `timerLoop` fires on `now + 10 > due` and re-arms
/// relative to `now`, so it self-corrects to 27.8 ticks a second (measured in
/// three scenes, ANALYSIS.md §31.9). On a 16.667 ms grid that 36 ms deadline
/// rounds up to 50 and the guest ran at 20/s — a third slower than it was
/// written for, which is what the user was seeing when they said the menus and
/// cutscenes were slow.
///
/// **12 ms, not 4.** `ceil(36 / 12) * 12 = 36` recovers the period exactly,
/// and a poll is not free: `updateScreen` (0x1520C4) calls
/// `Texture2D::update` unconditionally and every path through it ends in
/// `glDrawArrays`, so each poll costs one whole-canvas pass through
/// `hl5_core::gl` — 0.246 ms measured on a desktop x86 at 400x240. A 4 ms grid
/// would buy the same 36 ms and pay three times the blits for it. `gl {n} ms`
/// in the periodic line is that bill; watch it if this number ever changes.
const POLL: Duration = Duration::from_millis(12);

/// The fine grid the adaptive scheduler uses once a tick is due.
///
/// 2 ms, because that is where the sweep stopped paying: measured on the S25,
/// gameplay reached 26.6 repaints/s on a fixed 4 ms grid and 27.4 on a fixed
/// 2 ms one, against the guest's 27.8 design rate.
const FINE: Duration = Duration::from_millis(2);

/// How long before a tick is expected the fine grid starts.
///
/// It has to cover the jitter in *when* the guest's timer actually fires,
/// which is the wake latency (measured at 1-4 ms, the `late` figure) plus the
/// guest's own +/-1 tick of slack. 8 ms covers that with room and costs four
/// wakeups a tick; a wider window costs wakeups for nothing, and a narrower
/// one starts quantising again.
const FINE_LEAD: Duration = Duration::from_millis(8);

/// The tick rate the shell holds every device to, in Hz, when a cap is set.
///
/// # Why a cap exists at all
///
/// The guest moves the hero a fixed whole number of pixels **per tick** and
/// never scales by elapsed time (`HERO::WorkMoveHero` 0x8D020 is a pure
/// `(speed, angle)` function; `CHAR::ApplyMovePixel` applies it once and zeroes
/// it). So wall-clock speed is `pixels/tick x ticks/second`, and ticks/second is
/// whatever the phone can manage. Measured on the same build and the same save:
///
/// ```text
///   code's own ceiling   28.57 ticks/s   warrior 200 px/s
///   Galaxy S25           27.2            190
///   what the guest asks  22.2            155      (fps table entry 9 = 22)
///   Galaxy S9, cool      20.6            144
///   Galaxy S9, throttled 12.3             86
/// ```
///
/// The same character is 2.2x faster on an S25 than on a hot S9 — and it is not
/// only the hero: monsters, projectiles, animation and cooldowns are all
/// per-tick, so a slow phone runs the whole game in uniform slow motion. That
/// is faithful to 2011, when every phone was slow, but it means "movement
/// speed" is not a property of the game, it is a property of the hardware.
///
/// Capping the *served* tick rate makes it a property of the game again.
///
/// # Why 24
///
/// Above the 22.2 the guest's own timer asks for, so it is not a slowdown
/// against the original design; below the 27.8 ceiling, so a device with
/// headroom has some to spare for thermal droop. It also cuts work: on an S9
/// after the blitter ports a tick costs ~27 ms, so 24 Hz leaves the core ~35%
/// idle instead of pinned — and CPU saturation is what was collapsing that
/// phone's clock from 2.70 to 1.26 GHz in the first place.
const TICK_CAP_HZ: u32 = 24;

/// How often the CPU pin is re-asserted. See [`App::affinity_due`].
const AFFINITY_RETRY: Duration = Duration::from_secs(2);

/// `<files>/hl5_poll.txt` — the poll period in milliseconds, overriding
/// [`POLL`].
///
/// A switch rather than a constant because the constant was chosen from
/// arithmetic that the device then contradicted. `ceil(36 / 12) * 12 = 36`
/// only holds if the loop is woken *at* its deadline; measured on the S25 it
/// wakes **3.4 ms late** on average (the `late` figure in the periodic line),
/// so a nominal 12 ms grid is really a 15.4 ms one and the guest's 36 ms
/// deadline rounds up to 46. That is the whole reason gameplay sits at ~24
/// repaints/s while barely half the wall clock is busy. The right period is
/// therefore an empirical question about *this* device, and one that has to
/// be answerable without a reinstall.
const POLL_FILE: &str = "hl5_poll.txt";

struct App {
    guest: Vec<u8>,
    vfs: Arc<Vec<u8>>,
    storage: PathBuf,
    assets: PathBuf,
    /// Where the guest's `connect` is redirected, or `None` for the offline
    /// "the servers are gone" world. See [`read_server`].
    server: Option<(String, u16)>,
    /// This installation's identity, reported as `PHONENUMBER`. See
    /// [`read_account_id`].
    account_id: String,
    /// Whether boot runs the guest's own server dance or jumps past it. See
    /// [`read_boot_mode`].
    boot_mode: BootMode,
    /// How fast guest time runs against real time. See [`read_speed`].
    speed: f64,
    /// Whether the `HERO::KeyRelease` fix is installed. See
    /// [`read_keep_walking`].
    keep_walking: bool,
    boost_rates: bool,
    /// Whether to put the save in the boot log. See [`read_dump_save`].
    dump_save: bool,
    /// Whether the added cash items exist. See [`read_new_cash_items`].
    new_cash_items: bool,
    /// Ticks per second buff timers are armed for, 0 = the guest's own units.
    /// See [`read_skill_time`].
    skill_time_hz: u32,
    /// Which content line this build is. See [`read_vanilla`].
    vanilla: bool,
    /// The player's pad mapping, from `hl5_gamepad.txt`; empty when there is
    /// no such file, which is the normal case. See [`read_pad_map`].
    ///
    /// `None` is a binding too — the file's `= NONE`, "this button does
    /// nothing" — which is why this is a map of `Option` and not two
    /// collections. Splitting them is what made `NONE` beat a later binding
    /// for the same code regardless of what the player wrote.
    pad_map: std::collections::HashMap<i32, Option<hl5_core::gamepad::PadAction>>,
    /// Whether the badges show, from `hl5_pad_hud.txt`. See [`read_pad_hud`].
    pad_hud: hl5_core::padhud::PadHud,

    window: Option<Rc<Window>>,
    /// The GPU presenter, when one could be built. `None` because
    /// `hl5_present.txt` said `soft`, because EGL setup declined, or because
    /// a present failed mid-run — in every case [`App::present`] runs the
    /// softbuffer path below instead, which stays fully wired. See [`egl`]
    /// for why this exists (§33.0: present was 7.06 ms of a 36 ms budget).
    egl: Option<egl::EglPresenter>,
    /// Whether [`App::configure_surface`] should try EGL at all. Starts from
    /// [`read_present`]; cleared permanently for this run after any EGL
    /// failure, so a broken driver degrades once rather than every resume.
    want_egl: bool,
    /// Whether to confine the guest thread to the fastest CPU cluster. See
    /// [`affinity`] for why the scheduler will not do it unasked.
    pin_prime: bool,
    /// The last affinity note logged, so re-applying the same mask on every
    /// resume does not print the same line every time.
    affinity_note: String,
    /// When to (re-)apply the pin.
    ///
    /// Applying it once on `resumed` is not enough, and the device says so:
    /// coming back from the home screen logs
    /// `not pinned (sched_setaffinity(6-7) refused)`. `resumed` arrives while
    /// the process is still in the `background` cpuset (`0-1,4-5`), whose
    /// intersection with the prime cluster is empty — the kernel rejects the
    /// mask, and a one-shot attempt would leave the game running unpinned for
    /// the rest of its life over a race it lost by milliseconds. Re-applying
    /// on a slow timer costs one syscall and heals every cpuset transition,
    /// including ones nothing tells us about.
    affinity_due: Instant,
    context: Option<Context<Rc<Window>>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    /// Dimensions the surface was last configured to. The guest's framebuffer
    /// is sized once at boot; if the surface later disagrees (a rotation in
    /// flight, split-screen, a fold), presenting must rescale rather than
    /// memcpy rows of the wrong stride.
    surf_w: u32,
    surf_h: u32,
    vm: Option<Vm>,
    /// Set once a fault has been reported, so a broken guest logs once instead
    /// of once per frame.
    dead: bool,
    /// The guest called `CletEntry.exitApp()` (the 게임종료 menu item).
    quit: bool,
    next_frame: Instant,
    /// The [`POLL`] grid, kept apart from `next_frame` so the guest's tick rate
    /// stops being tied to the display's refresh rate.
    next_poll: Instant,
    last_tick: Instant,
    /// `vm.gl.draws` as of the last present.
    ///
    /// This was meant to answer "has the picture changed", on the reasoning
    /// that the guest paces itself with its own WIPI timers and never exceeds
    /// 22 fps while this loop ticks at the display rate. **Measured on the
    /// S25, that is wrong**: the guest issues a `glDrawArrays` on every single
    /// tick, so `draws` moved every tick, the test below never fired, and the
    /// log read `100% presented`. The guest's *timers* are capped; its
    /// re-blit is not.
    ///
    /// Kept as a cheap first filter — it costs one compare and does catch a
    /// tick where the guest drew nothing at all — with [`App::last_fb`] behind
    /// it doing the work. `u64::MAX` forces the first present.
    presented_gen: u64,
    /// The framebuffer as it was when it last reached the screen.
    ///
    /// `gl.draws` turned out to be a poor "has the picture changed" signal:
    /// measured on the S25, the guest issues a `glDrawArrays` on *every* shell
    /// tick, so draws/s == ticks/s and the skip below never fired (the log
    /// read `100% presented`). What actually changes at the guest's own pace
    /// is the content, and comparing 585x270 words costs a fraction of what
    /// scaling them to 2340x1080 and handing that to the compositor does.
    last_fb: Vec<u32>,
    /// Presents avoided by that comparison, for the periodic log.
    stat_skipped: u32,
    /// Whether [`App::present`] runs the canvas through Scale2x on its way to
    /// the window. See [`read_scale2x`].
    scale2x: bool,
    /// The 2x canvas, allocated once and reused. Empty when `scale2x` is off.
    doubled: Vec<u32>,
    /// The 해상도 setting this process booted with (`hl5_core::screen`), and
    /// the two canvas widths it produced: the one the guest was actually told
    /// about, and the one [`hl5_core::widened_canvas`] would have chosen on its
    /// own. Their ratio is the fraction of the window the picture may use —
    /// equal for mode `A`, which is what keeps the shipped path unchanged.
    screen_mode: hl5_core::ScreenMode,
    canvas_w: i32,
    full_canvas_w: i32,

    /// Rolling counters for the periodic performance line. This is the only
    /// way to know what the interpreter achieves on a given phone, which is
    /// the number that decides whether more guest functions need host
    /// implementations.
    stat_since: Instant,
    stat_frames: u32,
    /// Shell ticks in the window, and `gl.draws` at its start. The present-skip
    /// is only worth what the guest's draw rate leaves it: if the guest issues
    /// a `glDrawArrays` on every tick then `draws` moves every tick and nothing
    /// is skipped, which is invisible from the fps line alone (it counts
    /// presents, so it would just read as a high frame rate).
    stat_ticks: u32,
    stat_draws: u64,
    stat_steps: u64,
    t_guest: Duration,
    t_present: Duration,
    /// `t_present` split in three, because a total does not say which part to
    /// attack. On the softbuffer path `t_acquire` is `buffer_mut()` — asking
    /// the window for a buffer to write into; `t_blit` is our own scaling
    /// copy; `t_swap` is `present()`, handing that buffer to the compositor.
    /// On the EGL path there is nothing to acquire (`t_acquire` stays 0),
    /// `t_blit` is the texture upload and `t_swap` is the draw plus
    /// `eglSwapBuffers` — the same "our copy" / "handing it over" split, so
    /// the ten-second lines from the two paths compare like for like.
    ///
    /// The split matters because the reading it replaced was misleading:
    /// `acquire` and `present` used to be added together and called "swap",
    /// which invited reading the number as vblank wait. It is not — the
    /// display runs at 120 Hz while this app presents 23 times a second, so
    /// the BufferQueue never fills (ANALYSIS.md §31.3). Separated, `t_acquire`
    /// is the vendor softbuffer fork's dequeue and `t_swap` is its
    /// 2340x1080 staging swizzle, and those want different fixes.
    t_acquire: Duration,
    t_blit: Duration,
    t_swap: Duration,
    /// The poll period actually in force. See [`POLL_FILE`].
    poll: Duration,
    /// True between `suspended` and `resumed`, i.e. whenever there is no
    /// window to draw into.
    ///
    /// The loop used to keep offering the guest `nativeLoop` the whole time.
    /// Measured on the S25: a backgrounded run burns **84% of one core** —
    /// *more* than the 67% it uses in the foreground, because with no surface
    /// to present to, the loop comes round faster. `vm.pause()` does not stop
    /// it: that runs the guest's own `nativePauseApp`, and the guest's timers
    /// only advance because this shell keeps calling into them. So the game
    /// went on being played, invisibly, until Android got round to killing
    /// the process.
    asleep: bool,
    /// Whether to aim polls at the guest's own tick instead of running a
    /// fixed grid. See [`App::plan_next_poll`].
    adaptive: bool,
    /// When the guest last called `MC_grpFlushLcd`, and how far apart those
    /// calls have been running. The scheduler's whole model of the guest.
    last_repaint_at: Instant,
    last_repaint_count: u64,
    repaint_interval: Duration,
    /// Smallest interval between served ticks, or `None` for uncapped. See
    /// [`TICK_CAP_HZ`] and [`read_tick_cap`].
    tick_floor: Option<Duration>,
    /// The deadline handed to `ControlFlow::WaitUntil` last time round, and
    /// how far past it the loop actually woke.
    ///
    /// The two grids only mean what [`POLL`] and [`FRAME`] say if the loop is
    /// woken when it asked to be. `repaints/s` sitting at 24 while barely half
    /// the wall clock is busy is the signature of the *poll* period being
    /// longer than 12 ms, not of a CPU that ran out - and the difference
    /// between those two stories is this number.
    wake_due: Instant,
    t_late: Duration,
    /// `vm.gl.draw_time` at the start of the window, so the periodic line can
    /// report the host's upscale pass as a delta. This is the cost that scales
    /// with the *poll* rate rather than the present rate — the guest issues a
    /// `glDrawArrays` every `nativeLoop` whether the picture changed or not —
    /// which is the number the poll-grid decision turns on.
    stat_gl: Duration,
    /// `MC_grpFlushLcd` entries at the start of the window. The guest's own
    /// "repaint the screen" call, so this separates a tick that really redrew
    /// from a tick that ran the timer list and found nothing to do. Those are
    /// two different bills and the fps line cannot tell them apart.
    stat_flush: u64,

    /// Fingers currently down: winit's raw id, one slot each, with the last
    /// position seen. The slot index is what the guest is told.
    ///
    /// The guest stores the owning finger in a **signed byte**, so a raw winit
    /// id — a pointer-sized counter that keeps climbing — cannot survive the
    /// round trip. Slot indices make it exact by construction, and the release
    /// replay in [`App::release_all_touches`] covers the other half: winit
    /// delivers no release for a finger that was down when the activity was
    /// backgrounded, and a press whose release never arrives leaves the guest
    /// believing that finger is still on the screen.
    touch_ids: Vec<Option<(u64, i32, i32)>>,

    /// The platform audio bridge, or `None` when it could not be set up — a
    /// silent game rather than no game. See [`sound`].
    #[cfg(target_os = "android")]
    sound: Option<sound::Sound>,
    /// The document screens behind the 세이브 내보내기 / 불러오기 rows, or
    /// `None` when `Hl5Save` could not be reached — the rows then report
    /// failure. See [`savedoc`].
    #[cfg(target_os = "android")]
    savedoc: Option<savedoc::SaveDoc>,
}

impl App {
    /// Map a raw touch id onto a small stable slot, freeing it on release.
    fn touch_slot(&mut self, raw: u64, x: i32, y: i32, release: bool) -> i32 {
        let slot = match self.touch_ids.iter().position(|v| v.is_some_and(|(r, ..)| r == raw)) {
            Some(i) => i,
            None => match self.touch_ids.iter().position(Option::is_none) {
                Some(i) => i,
                None => {
                    self.touch_ids.push(None);
                    self.touch_ids.len() - 1
                }
            },
        };
        self.touch_ids[slot] = if release { None } else { Some((raw, x, y)) };
        // A phone will not report more than a handful at once, but the guest
        // keeps this in a signed byte, so it must never exceed 0x7f.
        (slot & 0x7f) as i32
    }

    /// Tell the guest every finger still down has lifted.
    ///
    /// Each replayed release carries its own slot, because the guest tracks
    /// which finger owns the touch joystick and only that one may let go of it
    /// (`StateGame::TouchReleased+0xdc`). A release replayed under the wrong id
    /// is a release the joystick ignores, and the stick would still be held
    /// down after the activity came back.
    ///
    /// Order does not matter, which is worth knowing because the guest's
    /// release mailbox is a single slot: `BaseState::TouchRelease` (0x44D28)
    /// latches `gv+0x126` when the *owning* finger lifts and then drops any
    /// further release until the tick is drained, so whichever order they go
    /// in, the owner's release is the one left in the mailbox.
    fn release_all_touches(&mut self) {
        let held: Vec<(i32, i32, i32)> = self
            .touch_ids
            .iter()
            .enumerate()
            .filter_map(|(slot, v)| v.map(|(_, x, y)| ((slot & 0x7f) as i32, x, y)))
            .collect();
        self.touch_ids.iter_mut().for_each(|v| *v = None);
        let Some(vm) = self.vm.as_mut() else { return };
        for (slot, x, y) in held {
            let _ = vm.touch(Touch::Released, x, y, slot);
        }
    }

    /// Build the VM against the real window size.
    ///
    /// Deferred until `resumed` because the guest is told its screen geometry
    /// exactly once, in `nativeInitDisplay`, and it keeps the framebuffer it
    /// allocates there.
    fn boot(&mut self, w: u32, h: u32) {
        if self.vm.is_some() || self.dead {
            return;
        }
        // A degenerate first size would be baked in forever: the canvas below
        // is derived from it, `nativeInitDisplay` is called once, and the
        // guest allocates its framebuffer from what it hears. `resumed` can
        // hand us a window whose size has not settled, so wait for the
        // `Resized` that follows — `boot` is idempotent, and the alternative
        // is a process running at the wrong aspect for its whole life.
        if w == 0 || h == 0 {
            log::info!("boot deferred: surface is {w}x{h}, waiting for a real size");
            return;
        }
        // Derived from the surface the guest is about to be told about, once:
        // `nativeInitDisplay` runs a single time and the guest allocates its
        // framebuffer from what it hears there, so this choice is permanent
        // for the process.
        let (wipi_w, wipi_h) =
            hl5_core::widened_canvas_for(w as i32, h as i32, self.screen_mode);
        // 세로's canvas is the whole of the box Java cut for it, so it is its
        // own full width and nothing is banded; the landscape modes measure
        // themselves against what `widened_canvas` would have picked.
        let full_w = if self.screen_mode.is_portrait() {
            wipi_w
        } else {
            hl5_core::widened_canvas(w as i32, h as i32).0
        };
        self.full_canvas_w = full_w;
        self.canvas_w = wipi_w;
        log::info!(
            "canvas: surface {w}x{h} -> logical {wipi_w}x{wipi_h} (해상도 {} = {}, full width {full_w})",
            self.screen_mode.letter(),
            self.screen_mode.label()
        );

        let cfg = Config {
            vfs: self.vfs.clone(),
            storage: self.storage.clone(),
            assets: self.assets.clone(),
            server: self.server.clone(),
            // With no server configured, the guest's connects are answered by
            // the in-process transcription of the private server
            // (`hl5_core::localserver`) — the cash shop's purchase round-trip
            // completes with no network and no INTERNET reachability at all.
            // A configured server always wins; `hl5_server.txt` still points
            // an installation at a real backend without a rebuild.
            local_server: self.server.is_none(),
            // …and on that installation the cash shop is free: the price line
            // is drawn struck through with `0원`, and a purchase grants the
            // item the game's own catalogue names for that row
            // (`shop::GrantTable`, CLAUDE.md "캐시샵을 로컬로"). This waited
            // on the kind/index mapping being *confirmed* rather than
            // guessed — switching it on earlier would have shown 0원 and left
            // the bag empty. `free_shop` implies `local_server`, so the two
            // cannot disagree; pointing an install at a real server turns
            // both off, which is the honest behaviour (a real backend, real
            // prices).
            free_shop: self.server.is_none(),
            // …and 창고접속 opens instead of bouncing the player out with a
            // server error. Same condition as the two above: an installation
            // pointed at a real backend gets the real conversation, including
            // the real failures.
            offline_menus: self.server.is_none(),
            // Not conditioned on the server, unlike the three above. Those
            // trade an absent operator for a stand-in, so a real backend has
            // to win; this one fixes a plain bug in the game —
            // `HERO::KeyRelease` (0x9BDCC) calls `NumKeyToNaviKey`, throws the
            // answer away, and stops the walk on *any* key release, so
            // attacking while moving is impossible (`hl5_core::input`). There
            // is no server, and no installation, for which the shipped
            // behaviour is the wanted one. It stays off in `Config::default`
            // only so the probe's 1,241-frame A/B has a way to run without it.
            // The one lever is [`read_keep_walking`], and it exists because
            // this is the only behaviour change here that has never run on a
            // device.
            keep_walking: self.keep_walking,
            boost_rates: self.boost_rates,
            dump_save: self.dump_save,
            new_cash_items: self.new_cash_items,
            skill_time_hz: self.skill_time_hz,
            // One switch for both halves of §71: the operator decided the
            // rebalance in real seconds, so neither is wanted without the other.
            buff_rebalance: self.skill_time_hz > 0,
            vanilla: self.vanilla,
            // Gamepad support, unconditionally on: a pad that is not plugged
            // in costs one declining observer hook per drawn frame and paints
            // nothing, and there is no installation for which "ignore the
            // pad" is the wanted behaviour. What the *player* can turn off is
            // the badges (`hl5_pad_hud.txt`) and any individual binding
            // (`hl5_gamepad.txt`). It stays off in `Config::default` only so
            // the probe's 1,241-frame A/B has a way to run without it.
            gamepad: true,
            // The `*아바타` button on the character page and the wardrobe
            // behind it (`hl5_core::avatar`). Nothing changes until the player
            // opens it and picks a look; it stays off in `Config::default`
            // only so the 1,241-frame A/B has a way to run without it.
            avatar: true,
            // 가방·창고 10페이지 (`hl5_core::tenpages`): the bag and the
            // 공용창고 hold 160 slots. Not conditioned on the server — the
            // bag is a local save either way, and a real server's warehouse
            // still says how big it is (the object just has room to spare).
            // A save holding more than 96 items cannot be opened by a build
            // without this; ANALYSIS.md §65.6.
            ten_pages: true,
            shop_grants: Vec::new(),
            account_id: self.account_id.clone(),
            boot_mode: self.boot_mode,
            speed: self.speed,
            // The guest's screen *is* the canvas, and the canvas is already
            // the display's shape ([`hl5_core::widened_canvas`]) — so
            // magnification is 1.0, margins are 0, and the scale
            // [`App::present`] applies comes out uniform on both axes — and,
            // since `widened_canvas` now picks a height that divides the
            // surface, a whole number: 2340/585 = 1080/270 = 4 exactly on this
            // phone, so every canvas pixel is a 4x4 block rather than being
            // smeared over four rows here and five there. Telling the guest the
            // raw 400x240 and letting present stretch it, as this shipped until
            // §27, made every sprite 30% too wide.
            //
            // Keeping the framebuffer at canvas size rather than surface size
            // is deliberate: it is what removes the full-window sampling pass
            // (125 K pixels instead of 2.4 M) that cost half the frame before
            // commit b26b201. The sibling chronosSword sizes it to the
            // surface instead; that is the arrangement this project already
            // measured and rejected.
            screen_w: wipi_w,
            screen_h: wipi_h,
            wipi_w,
            wipi_h,
            // The row on 환경설정 that owns the two lines above, and the value
            // it is currently showing. See `hl5_core::screen`.
            screen_mode: self.screen_mode,
            screen_option: true,
            scale2x: self.scale2x,
            // Wall clock on the device: the guest's timers should fire at the
            // rate a player experiences, not at whatever a replay chose.
            virtual_clock: false,
            ..Config::default()
        };
        let mut vm = match Vm::new(&self.guest, cfg) {
            Ok(v) => v,
            Err(e) => {
                log::error!("guest load failed: {e}");
                self.dead = true;
                return;
            }
        };
        log::info!("guest loaded: {} symbols, {} imports", vm.elf.symbols.len(), vm.imports.len());
        #[cfg(target_os = "android")]
        if let Some(d) = self.savedoc.as_ref() {
            vm.savexfer.utc_offset_min = d.utc_offset_min;
        }
        // The pad mapping and the badge preference, before `boot` logs which
        // of them is in force. `Vm::pad_present` comes later, from Java.
        vm.pad.hud = self.pad_hud;
        vm.pad.overrides.extend(self.pad_map.iter().map(|(c, a)| (*c, *a)));
        if !vm.pad.overrides.is_empty() {
            log::info!(
                "pad: {} binding(s) from {}",
                vm.pad.overrides.len(),
                PAD_MAP_FILE
            );
        }
        // The host blitters. Without them the interpreter is the whole frame
        // budget; `patch.rs` explains which functions and why, and the probe's
        // `--no-patch` is the reference half of the A/B that keeps them honest.
        match vm.install_patches() {
            Ok(on) => log::info!("host patches: {}", on.join(", ")),
            Err(e) => log::error!("host patches not installed: {e}"),
        }
        if std::env::var("HL5_PROFILE").is_ok_and(|v| v != "0") {
            log::info!("guest profiling enabled");
            vm.profile = Some(Default::default());
        }
        if let Err(e) = vm.boot() {
            log::error!("guest boot failed after {} instructions: {e}", vm.steps);
            write_report(&self.storage, &format!("boot failed after {} steps\n\n{e}\n", vm.steps));
            self.dead = true;
            return;
        }
        let (_, fw, fh) = vm.framebuffer();
        log::info!("boot ok in {} guest instructions; surface {w}x{h}, guest {fw}x{fh}", vm.steps);
        // Boot's own notes — which patches and free-shop hooks were installed,
        // and whether a grant table was built — belong next to the boot line
        // rather than a frame later, mixed into gameplay.
        for line in vm.log.drain(..) {
            log::info!("{line}");
        }
        self.vm = Some(vm);
        self.last_tick = Instant::now();
    }

    /// Confine the guest thread to the fastest cluster, and say so once.
    ///
    /// Called from `resumed`, which runs on the guest thread — the affinity
    /// call only works on the calling thread ([`affinity`]). Repeats are free
    /// and are what makes the pin survive a background round trip, so the log
    /// is suppressed unless the answer changed.
    fn apply_affinity(&mut self) {
        if !self.pin_prime {
            if self.affinity_note.is_empty() {
                self.affinity_note = "off".to_string();
                log::info!("affinity: off; the scheduler places the guest thread");
            }
            return;
        }
        let note = match affinity::pin_to_fastest() {
            Ok(note) => note,
            Err(why) => format!("not pinned ({why})"),
        };
        if note != self.affinity_note {
            log::info!("affinity: {note}");
            self.affinity_note = note;
        }
        // A resume normally reports the same mask as before, which the test
        // above swallows. The interesting case is the one that *changed*, and
        // that is exactly what gets printed.
    }

    /// Point the presenter at the window: EGL when wanted and available,
    /// softbuffer otherwise. Called from `resumed` (fresh window) and from
    /// `Resized`; both presenters are built lazily here so an EGL failure at
    /// any point degrades to the CPU path instead of a black screen.
    fn configure_surface(&mut self, w: u32, h: u32) {
        let Some(win) = self.window.as_ref() else {
            return;
        };
        if self.want_egl && self.egl.is_none() {
            match egl::EglPresenter::new(win) {
                Ok(p) => {
                    log::info!("presenter: {}", p.describe());
                    self.egl = Some(p);
                }
                Err(e) => {
                    log::warn!("EGL presenter unavailable ({e}); using softbuffer");
                    self.want_egl = false;
                }
            }
        }
        if self.egl.is_some() {
            // The EGL window surface tracks the ANativeWindow by itself; the
            // dimensions are only remembered for the touch inverse
            // ([`App::to_guest`]) — the viewport is queried from EGL at
            // present time, where it is authoritative.
            if w != 0 && h != 0 {
                self.surf_w = w;
                self.surf_h = h;
            }
            return;
        }
        // The CPU path. The softbuffer Context is created here rather than in
        // `resumed` so a mid-run fallback from EGL can stand it up too.
        if self.context.is_none() {
            match Context::new(win.clone()) {
                Ok(c) => self.context = Some(c),
                Err(e) => {
                    log::error!("softbuffer Context::new failed: {e}");
                    return;
                }
            }
        }
        let (Some(ctx), Some(win)) = (self.context.as_ref(), self.window.as_ref()) else {
            return;
        };
        if self.surface.is_none() {
            match Surface::new(ctx, win.clone()) {
                Ok(s) => self.surface = Some(s),
                Err(e) => {
                    log::error!("Surface::new failed: {e}");
                    return;
                }
            }
        }
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
            return;
        };
        if let Some(s) = self.surface.as_mut() {
            if let Err(e) = s.resize(nw, nh) {
                log::error!("surface resize failed: {e}");
            } else {
                self.surf_w = w;
                self.surf_h = h;
            }
        }
    }

    /// The guest went outside its address space, which is where a run ends.
    ///
    /// Setting `dead` and returning is worse than it sounds: the last frame
    /// stays on the screen for ever, no input does anything, and the player is
    /// left looking at a game that has simply stopped — with the diagnosis in
    /// a logcat buffer they will never read. So write the report where they
    /// can reach it, and let the process go. Restarting loses nothing that was
    /// not already lost: the VM's memory is inconsistent by definition here.
    fn crashed(&mut self, what: &str, err: &str) {
        if self.dead {
            return;
        }
        self.dead = true;
        let steps = self.vm.as_ref().map_or(0, |v| v.steps);
        log::error!("guest fault during {what} after {steps} instructions: {err}");
        write_report(
            &self.storage,
            &format!("guest fault during {what}\nafter {steps} instructions\n\n{err}\n"),
        );
        self.quit = true;
    }

    /// Hand the frame's audio calls to the platform.
    ///
    /// Between the guest step and the present on purpose: the events were
    /// produced by the step just above, and a sound the guest starts on the
    /// same frame as the picture it belongs to should not wait for the next
    /// one.
    #[cfg(target_os = "android")]
    fn audio_flush(&mut self) {
        let Some(vm) = self.vm.as_mut() else { return };
        let events = vm.audio.take_events();
        match self.sound.as_mut() {
            Some(s) => s.apply(&events),
            // Nothing to forward them to. Draining anyway is deliberate: the
            // queue is unbounded, and a run with no audio bridge would
            // otherwise accumulate every call the guest ever made.
            None => {}
        }
    }

    #[cfg(not(target_os = "android"))]
    fn audio_flush(&mut self) {
        if let Some(vm) = self.vm.as_mut() {
            vm.audio.take_events();
        }
    }

    #[cfg(target_os = "android")]
    fn audio_suspend(&mut self) {
        if let Some(s) = self.sound.as_mut() {
            s.suspend();
        }
    }

    #[cfg(not(target_os = "android"))]
    fn audio_suspend(&mut self) {}

    #[cfg(target_os = "android")]
    fn audio_restore(&mut self) {
        if let Some(s) = self.sound.as_mut() {
            s.restore();
        }
    }

    #[cfg(not(target_os = "android"))]
    fn audio_restore(&mut self) {}

    /// The save rows' traffic with the phone's document screens: an answer
    /// that arrived from Java goes to the VM, and a new request goes to Java.
    /// A request that cannot reach Java is answered as failed here, so the
    /// row shows the game's own failure message rather than waiting forever.
    #[cfg(target_os = "android")]
    fn save_doc_flush(&mut self) {
        let Some(vm) = self.vm.as_mut() else { return };
        if let Some(code) = SAVE_DOC_RESULT.lock().ok().and_then(|mut r| r.take()) {
            vm.save_doc_done(hl5_core::savexfer::DocStatus::from_code(code));
        }
        if let Some(req) = vm.take_save_doc_request() {
            let sent = self.savedoc.as_ref().is_some_and(|d| d.request(&req));
            if !sent {
                vm.save_doc_done(hl5_core::savexfer::DocStatus::Failed);
            }
        }
    }

    #[cfg(not(target_os = "android"))]
    fn save_doc_flush(&mut self) {
        if let Some(vm) = self.vm.as_mut() {
            if vm.take_save_doc_request().is_some() {
                vm.save_doc_done(hl5_core::savexfer::DocStatus::Failed);
            }
        }
    }

    /// When to offer the guest its next `nativeLoop`.
    ///
    /// # Why not simply a fine grid
    ///
    /// The guest ticks on its own ~36 ms timer and can only be *served* on a
    /// poll, so the poll period is both a ceiling and a quantiser on its tick
    /// rate. A fixed 12 ms grid was chosen for `ceil(36 / 12) * 12 = 36`, but
    /// the device then woke the loop 3.4 ms late on average, making the real
    /// grid 15.4 ms and the served period 46 — gameplay at 23.4 repaints/s
    /// against a design 27.8, with barely half the wall clock busy. Dropping
    /// the grid to a fixed 2 ms does reach 27.4, and costs 318 wake-ups a
    /// second to do it: measured, +15 points of a core (54% -> 69%) for +14%
    /// of game speed, because a wake-up through the event loop is ~0.8 ms of
    /// CPU whatever it finds.
    ///
    /// # What this does instead
    ///
    /// Sleep coarsely until the tick is nearly due, then use the fine grid
    /// only across the window where it can actually fire. The guest announces
    /// each tick by calling `MC_grpFlushLcd`, so the interval is *observed*
    /// rather than assumed — which is what makes this safe in the prologue,
    /// where the guest pins its timer at 55 ms (§16.6), and in any scene that
    /// changes it.
    ///
    /// If the guest stops repainting altogether the `now >= target` branch
    /// takes over and this degenerates to exactly the fixed fine grid — the
    /// configuration that was measured to work, so the failure mode is a cost,
    /// never a stall.
    fn plan_next_poll(&self, started: Instant, now: Instant) -> Instant {
        if !self.adaptive {
            // Measured from where the tick *started*, which is what the fixed
            // grid always meant: a scene the guest cannot keep up with then
            // runs flat out instead of adding the tick's own cost to every
            // period. Getting this wrong turns the control arm of an A/B into
            // a different experiment — 24 polls a second instead of 33.
            return started + self.poll;
        }
        // The cap raises the *aim*, never the observed interval itself: the
        // EWMA keeps telling the truth about what the guest is doing, and only
        // the moment we next serve it moves. That matters in the prologue,
        // where the guest pins its own timer at 55 ms — there the observed
        // interval is already the larger of the two and the cap does nothing.
        let target = match self.tick_floor {
            // A cap that is SLOWER than what the guest is already managing is
            // the whole point, and it has to be aimed at exactly, with no
            // `FINE_LEAD`. Subtracting the lead here was measured to do
            // nothing at all: opening the fine-grid window 8 ms early lets the
            // guest fire on its own ~36 ms timer before the cap's 41.7 ms is
            // up, so both phones stayed at 27.x with a 24 Hz cap set. The lead
            // exists to stop the scheduler *missing* a tick the guest wants
            // now; when we are deliberately making it wait, it is backwards.
            Some(floor) if floor > self.repaint_interval => self.last_repaint_at + floor,
            // Cap slower than the guest, or none: unchanged behaviour.
            _ => self.last_repaint_at + self.repaint_interval.saturating_sub(FINE_LEAD),
        };
        if now < target {
            target
        } else {
            now + self.poll
        }
    }

    /// Give the guest one `nativeLoop`, when [`App::plan_next_poll`] says.
    /// Hand the winit thread's pad events to the VM. Called at the top of a
    /// tick, so an edge that arrived while this loop was parked is in the
    /// guest's mailbox before the `nativeLoop` that drains it.
    fn drain_pad(&mut self) {
        // The VM check comes FIRST, and taking the queue second, because Java
        // reports the pad from `onCreate` — before `resumed` has built a VM
        // for it to be reported to. Draining into nothing there would throw
        // away the one "a pad is attached" a player who plugged in before
        // launching ever gets, and the badges would stay off until they
        // unplugged it and plugged it back in.
        if self.vm.is_none() {
            return;
        }
        let events: Vec<PadEvent> = match PAD_QUEUE.lock() {
            Ok(mut q) if !q.is_empty() => std::mem::take(&mut *q),
            _ => return,
        };
        let Some(vm) = self.vm.as_mut() else { return };
        for ev in events {
            match ev {
                PadEvent::Button { code, down, repeat } => vm.pad_button(code, down, repeat),
                PadEvent::Axes { x, y, hx, hy, lt, rt } => vm.pad_axes(x, y, hx, hy, lt, rt),
                PadEvent::Present(p) => vm.pad_present(p),
            }
        }
    }

    fn tick(&mut self) {
        if self.dead {
            return;
        }
        self.drain_pad();
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick).as_millis() as i64;
        self.last_tick = now;

        let Some(vm) = self.vm.as_mut() else { return };
        let t0 = Instant::now();
        if let Err(e) = vm.frame(dt.clamp(1, 250)) {
            self.crashed("frame", &e);
            return;
        }
        self.t_guest += t0.elapsed();

        // The scheduler's only input. `guest_repaints` counts
        // `MC_grpFlushLcd`, which the guest calls once per game tick whether
        // or not the picture changed, so it is a clean clock on the thing
        // being scheduled for.
        let repaints = vm.guest_repaints();
        if repaints != self.last_repaint_count {
            self.last_repaint_count = repaints;
            let seen = now.saturating_duration_since(self.last_repaint_at);
            self.last_repaint_at = now;
            // A quarter-weight EWMA: fast enough to follow a scene change
            // (36 ms play, 55 ms prologue) within a few ticks, slow enough
            // that one late wake-up does not move the schedule. Clamped
            // because a pause — a save, a modal that stops the timer — would
            // otherwise teach it an interval that is not a tick rate.
            let seen = seen.clamp(Duration::from_millis(8), Duration::from_millis(120));
            self.repaint_interval = (self.repaint_interval * 3 + seen) / 4;
        }

        // Everything the host layers say about themselves goes into `vm.log`
        // — the private server's login/purchase/grant lines, the free-shop
        // price geometry, `--watch` hits. The probe prints that vector when a
        // run ends; on a device nobody ever read it, so it grew without bound
        // and every one of those diagnostics was invisible in `logcat -s hl5`.
        // Drain it here: it is the only channel that says *why* a purchase
        // granted nothing, and on a phone there is no other way to ask.
        for line in vm.log.drain(..) {
            log::info!("{line}");
        }

        // The 화면필터 row moved: switch the presenter's Scale2x pass. The
        // doubled buffer is dropped (its size no longer matches) and the next
        // present is forced, because the canvas itself has not changed and
        // the content compare would otherwise keep the old picture up.
        if let Some(on) = vm.take_scale2x_change() {
            log::info!("filter: {}", if on { "scale2x" } else { "nearest" });
            self.scale2x = on;
            self.doubled = Vec::new();
            self.last_fb.clear();
            self.presented_gen = u64::MAX;
        }

        if vm.exit_requested && !self.quit {
            log::info!("guest asked to exit; closing");
            self.quit = true;
            return;
        }

        self.audio_flush();
        self.save_doc_flush();
        self.stat_ticks += 1;

        if self.stat_since.elapsed() >= Duration::from_secs(10) {
            let secs = self.stat_since.elapsed().as_secs_f64();
            let vm = self.vm.as_ref().expect("present ran");
            let steps = vm.steps - self.stat_steps;
            let repaints = vm.guest_repaints() - self.stat_flush;
            // `ticks/s` counts **host loop iterations**, not guest game ticks.
            // Those two were read as one number once, and it made a CPU-bound
            // scene look as though it had reached the guest's design ceiling
            // (ANALYSIS.md §31.9). `repaints/s` is the guest-side rate —
            // `MC_grpFlushLcd` entries — and the gap between the two is the
            // polls on which the guest found nothing to do.
            //
            // `gl` is the host's own upscale pass, which runs once per *poll*
            // rather than once per present: `updateScreen` calls
            // `glDrawArrays` unconditionally. It is therefore the cost that
            // grows if this loop polls more often, and it is invisible in
            // `present` — which is why the two are reported apart.
            log::info!(
                "{:.1} fps, {:.1} M guest ins/s (guest {:.0} ms, present {:.0} ms per 10 s); \
                 {:.0} ticks/s, {:.1} repaints/s, {:.0} draws/s ({} total, {} elided), {} skipped; \
                 gl {:.0} ms, acquire {:.0} ms, blit {:.0} ms, swap {:.0} ms, late {:.0} ms; \
                 heap {} KB live / {} KB peak",
                self.stat_frames as f64 / secs,
                steps as f64 / secs / 1.0e6,
                self.t_guest.as_secs_f64() * 1000.0,
                self.t_present.as_secs_f64() * 1000.0,
                self.stat_ticks as f64 / secs,
                repaints as f64 / secs,
                (vm.gl.draws - self.stat_draws) as f64 / secs,
                vm.gl.draws,
                vm.gl.draws_elided,
                self.stat_skipped,
                (vm.gl.draw_time - self.stat_gl).as_secs_f64() * 1000.0,
                self.t_acquire.as_secs_f64() * 1000.0,
                self.t_blit.as_secs_f64() * 1000.0,
                self.t_swap.as_secs_f64() * 1000.0,
                self.t_late.as_secs_f64() * 1000.0,
                vm.heap.live_bytes / 1024,
                vm.heap.peak_bytes / 1024,
            );
            self.stat_steps = vm.steps;
            self.stat_draws = vm.gl.draws;
            self.stat_gl = vm.gl.draw_time;
            self.stat_flush = vm.guest_repaints();
            self.stat_frames = 0;
            self.stat_ticks = 0;
            self.stat_since = Instant::now();
            self.t_guest = Duration::ZERO;
            self.t_present = Duration::ZERO;
            self.t_acquire = Duration::ZERO;
            self.t_blit = Duration::ZERO;
            self.t_swap = Duration::ZERO;
            self.t_late = Duration::ZERO;
            self.stat_skipped = 0;
        }
    }

    /// Offer the guest's framebuffer to the compositor, on the [`FRAME`] grid.
    ///
    /// `gl.draws` counts `glDrawArrays` and moves on *every* poll —
    /// `updateScreen` issues one unconditionally — so it was never the right
    /// question. `gl.fb_gen` is: it moves only when the host's composite pass
    /// actually rewrote the framebuffer, which it now declines to do when the
    /// draw would reproduce what is already there. That makes this filter
    /// cheap *and* meaningful, and it spares the whole-framebuffer compare in
    /// [`App::present`] on the polls where nothing could have changed.
    /// Resizes and system redraws go through `RedrawRequested`, which presents
    /// unconditionally.
    fn present_if_new(&mut self) {
        if self.dead {
            return;
        }
        let gen = self.vm.as_ref().map_or(0, |v| v.gl.fb_gen);
        if gen == self.presented_gen {
            return;
        }
        let t1 = Instant::now();
        self.present();
        self.t_present += t1.elapsed();
        self.presented_gen = gen;
    }

    /// Double the canvas with **Scale2x**, into `out`.
    ///
    /// # Why a pixel-art scaler and not a smooth one
    ///
    /// This game's content is 2011 feature-phone art: 92.9% of its 4,795
    /// images are 16-colour, the modal sprite is 20x20, transparency is a
    /// single key colour, and both fonts are 1-bit bitmaps 11 pixels tall.
    /// Magnified 4.5x by [`App::present`], every glyph edge and every sprite
    /// key edge becomes a staircase — and that staircase, not the 4.5 factor
    /// itself, is what reads as "low quality".
    ///
    /// A smooth filter is the wrong tool for that input, and it was measured
    /// rather than assumed. Bilinear at this factor keeps block interiors, but
    /// it invents colour on **24% of text pixels and 61% of scene pixels**,
    /// drops a 1-pixel stroke's peak to 240/255, and costs the CPU 15.3 ms a
    /// frame against a ~43 ms budget. xBR-class kernels cost 6.8 ms. Scale2x
    /// costs **0.20 ms**, invents **no colour at all** — every output pixel is
    /// one of the four inputs — touches 2.0–4.3% of the picture, leaves every
    /// stem and every gap exactly as authored, and halves the staircase
    /// density (text 2.31% -> 1.26% of pixels, gameplay 7.35% -> 3.55%).
    ///
    /// The whole two-pass path benches at roughly what today's single nearest
    /// pass costs, because the second pass is unchanged and the first one is
    /// cheap; on the device expect the ten-second line's `blit` to go from
    /// ~1.0 ms to ~1.3 ms.
    ///
    /// # The rule
    ///
    /// For a source pixel `P` and its four edge neighbours, an output quadrant
    /// takes a neighbour only where two adjacent neighbours agree *and* the
    /// opposing pair does not — i.e. exactly at a diagonal edge:
    ///
    /// ```text
    ///       A            E0 E1        E0 = C==A && C!=D && A!=B ? A : P
    ///     C P B   ->     E2 E3        E1 = A==B && A!=C && B!=D ? B : P
    ///       D                         E2 = D==C && D!=B && C!=A ? C : P
    ///                                 E3 = B==D && B!=A && D!=C ? D : P
    /// ```
    ///
    /// Edge pixels take themselves as their missing neighbours, which leaves
    /// the border untouched rather than inventing one.
    fn scale2x(src: &[u32], w: usize, h: usize, out: &mut Vec<u32>) {
        let (dw, dh) = (w * 2, h * 2);
        if out.len() != dw * dh {
            out.clear();
            out.resize(dw * dh, 0);
        }
        if w == 0 || h == 0 || src.len() < w * h {
            return;
        }
        for y in 0..h {
            let row = y * w;
            let up = if y > 0 { row - w } else { row };
            let down = if y + 1 < h { row + w } else { row };
            let o0 = (y * 2) * dw;
            let o1 = o0 + dw;
            for x in 0..w {
                let p = src[row + x];
                let a = src[up + x];
                let d = src[down + x];
                let c = if x > 0 { src[row + x - 1] } else { p };
                let b = if x + 1 < w { src[row + x + 1] } else { p };
                let (e0, e1, e2, e3) = if a == d || c == b {
                    // The Scale2x short circuit: with either axis uniform no
                    // quadrant can qualify, and this is the overwhelmingly
                    // common case in flat 16-colour art.
                    (p, p, p, p)
                } else {
                    (
                        if c == a { a } else { p },
                        if a == b { b } else { p },
                        if d == c { c } else { p },
                        if b == d { d } else { p },
                    )
                };
                out[o0 + x * 2] = e0;
                out[o0 + x * 2 + 1] = e1;
                out[o1 + x * 2] = e2;
                out[o1 + x * 2 + 1] = e3;
            }
        }
    }

    /// Where the canvas lands across the window, in window pixels: the left
    /// edge and the width.
    ///
    /// The arithmetic itself lives in [`hl5_core::screen::letterbox`], where it
    /// has tests: the probe has no window, so the round trip
    /// canvas -> window -> finger is the one part of the 해상도 setting that
    /// an offline replay cannot exercise. This is the only caller-side wrapper,
    /// and [`App::present`], the EGL viewport and [`App::to_guest`] all read it.
    fn dest_x(&self, sw: usize) -> (usize, usize) {
        let (x0, w) =
            hl5_core::screen::letterbox(sw as i32, self.canvas_w, self.full_canvas_w);
        (x0.max(0) as usize, w.max(1) as usize)
    }

    /// Scale the guest framebuffer over the whole window.
    ///
    /// Two presenters, one contract. The front half — the content compare,
    /// the frame accounting, Scale2x — is shared; then the frame goes to the
    /// GPU as a texture when the EGL presenter is up ([`egl`]), and through
    /// the fixed-point CPU loop below when it is not. The loop is not dead
    /// code behind a preference: it is the fallback the EGL path retires
    /// itself onto, and the path a desktop host always takes.
    ///
    /// The GL capture layer's framebuffer is the logical canvas (see
    /// [`App::boot`]), and this is the one place it meets the display.
    /// The scale is uniform in practice: the canvas was chosen to have the
    /// display's aspect ([`hl5_core::widened_canvas`]), so the two ratios
    /// below agree — exactly on a 2340x1080 (4.5 and 4.5), and to within the
    /// flooring residue elsewhere. Computing them per axis rather than
    /// asserting one keeps a surface that disagrees with the framebuffer — a
    /// rotation or fold in flight — showing a squashed picture rather than
    /// reading off the end of a row.
    ///
    /// Historically this really did scale the axes independently — fill, not
    /// letterbox, every window pixel written, which is also why there is no
    /// clear before the loop. Fixed point keeps the inner loop at an add and
    /// a table load per pixel; the row's source indices depend only on `x`,
    /// so they are computed once per present rather than once per row.
    fn present(&mut self) {
        let Some(vm) = self.vm.as_ref() else { return };
        if self.egl.is_none() && self.surface.is_none() {
            return;
        }
        let (fb, fw, fh) = vm.framebuffer();
        // Nothing changed since the buffer on screen was built: the scaling
        // copy would write the same 2.3 M words and the compositor would wait
        // out another vblank to show the same picture.
        if self.last_fb.len() == fb.len() && self.last_fb == fb {
            self.stat_skipped += 1;
            return;
        }
        // Counted here rather than at the call site so the reported rate is
        // pictures actually put on the screen. Counting attempts instead read
        // as ~58 fps while three quarters of them were being dropped as
        // identical — a number that cannot go down no matter what changes.
        self.stat_frames += 1;
        self.last_fb.clear();
        self.last_fb.extend_from_slice(fb);

        // Double the canvas with Scale2x before the scale to the window. See
        // [`App::scale2x`]; `self.doubled` owns the buffer so it is allocated
        // once for the life of the process. The comparison above is on the
        // *source*, which is both cheaper and the right question.
        let (fb, fw, fh) = if self.scale2x {
            let (w, h) = (fw.max(1) as usize, fh.max(1) as usize);
            Self::scale2x(fb, w, h, &mut self.doubled);
            (&self.doubled[..], (w * 2) as i32, (h * 2) as i32)
        } else {
            (fb, fw, fh)
        };

        // The GPU path: hand the (doubled) canvas to GLES2 as one texture and
        // let the sampler do the scale to the window. Nearest at fragment
        // centers agrees with the edge-sampling table loop below on every
        // pixel when the ratio is an integer — the shipped case, by §33's
        // construction of the canvas — and can differ by a one-texel phase
        // when it is not (see [`egl`]). The CPU path stays wired underneath:
        // a failure here retires the presenter for the rest of the run and
        // the next frame goes through softbuffer.
        if let Some(p) = self.egl.as_mut() {
            match p.present(fb, fw, fh, (self.canvas_w, self.full_canvas_w)) {
                Ok((upload, swap)) => {
                    self.t_blit += upload;
                    self.t_swap += swap;
                }
                Err(e) => {
                    log::warn!("EGL present failed ({e}); falling back to softbuffer");
                    self.egl = None;
                    self.want_egl = false;
                    let (w, h) = (self.surf_w, self.surf_h);
                    self.configure_surface(w, h);
                    // The frame that just failed never reached the screen;
                    // forget it so the next FRAME tick presents it through
                    // the fallback instead of skipping it as already-shown.
                    self.last_fb.clear();
                }
            }
            return;
        }
        let (fw, fh) = (fw.max(1) as u64, fh.max(1) as u64);
        let (sw, sh) = (self.surf_w.max(1) as usize, self.surf_h.max(1) as usize);
        // The 해상도 setting: the picture occupies `dw` of the window's `sw`
        // columns, starting at `x0`. Full width gives `(0, sw)` and every
        // expression below reduces to what it was.
        let (x0, dw) = self.dest_x(sw);
        let step_x = (fw << 16) / dw as u64;
        let step_y = (fh << 16) / sh as u64;

        let mut cols: Vec<u32> = Vec::with_capacity(dw);
        let mut sx: u64 = 0;
        for _ in 0..dw {
            cols.push(((sx >> 16).min(fw - 1)) as u32);
            sx += step_x;
        }

        let t_acquire = Instant::now();
        let surface = self.surface.as_mut().expect("checked above");
        let mut buf = match surface.buffer_mut() {
            Ok(b) => b,
            Err(e) => {
                log::error!("buffer_mut failed: {e}");
                return;
            }
        };
        self.t_acquire += t_acquire.elapsed();
        let t_blit = Instant::now();
        // Most destination rows are duplicates. At 4x each source row feeds
        // four of them, so only 270 of 1080 rows need the gather; the rest are
        // a straight copy of the row just built, which is a memcpy rather than
        // 2,340 table lookups. (With Scale2x in front the gather runs over the
        // doubled 1170x540 canvas and each row feeds two.)
        let rows = sh.min(buf.len() / sw);
        let mut prev_sy = usize::MAX;
        let mut prev_row = 0usize;
        for y in 0..rows {
            let sy = ((y as u64 * step_y) >> 16).min(fh - 1) as usize;
            if sy == prev_sy {
                let (done, rest) = buf.split_at_mut(y * sw);
                rest[..sw].copy_from_slice(&done[prev_row..prev_row + sw]);
                continue;
            }
            let src = &fb[sy * fw as usize..(sy + 1) * fw as usize];
            let row = &mut buf[y * sw..(y + 1) * sw];
            // The bands beside a narrowed canvas. Written every row rather
            // than once, because softbuffer hands back a recycled buffer whose
            // contents are whatever was in it two frames ago.
            row[..x0].fill(0);
            row[x0 + dw..].fill(0);
            for (px, &c) in row[x0..x0 + dw].iter_mut().zip(&cols) {
                *px = src[c as usize];
            }
            prev_sy = sy;
            prev_row = y * sw;
        }
        self.t_blit += t_blit.elapsed();
        let t_swap = Instant::now();
        if let Err(e) = buf.present() {
            log::error!("present failed: {e}");
        }
        self.t_swap += t_swap.elapsed();
    }

    /// Where a touch in window pixels lands on the guest's canvas.
    ///
    /// The exact inverse of [`App::present`], computed the same way per axis
    /// so the two cannot drift: whatever ratio put a pixel on screen is the
    /// ratio that brings a finger back. The guest itself was told
    /// magnification 1.0 and margins 0, so `Vm::map_touch` —
    /// `CletEntry.onTouchHandleEvent`'s subtract/divide/clamp — degenerates
    /// to the clamp, and the scale has exactly one owner: a second one living
    /// inside the guest's numbers would double-apply.
    fn to_guest(&self, x: f64, y: f64) -> (i32, i32) {
        let Some(vm) = self.vm.as_ref() else { return (0, 0) };
        let (_, fw, fh) = vm.framebuffer();
        let (x0, dw) = self.dest_x(self.surf_w.max(1) as usize);
        let sh = self.surf_h.max(1) as f64;
        // A finger in the black band clamps to the nearest edge, which
        // `Vm::map_touch` already does; there is nothing there to press.
        vm.map_touch((x - x0 as f64) * fw as f64 / dw as f64, y * fh as f64 / sh)
    }
}

impl ApplicationHandler<PadWake> for App {
    /// A pad event arrived on the Java thread and woke this one.
    ///
    /// This pulls the next poll forward and does **not** tick: the guest paces
    /// itself on its own timers, so an extra `nativeLoop` mostly finds nothing
    /// due, and an analog stick streams samples at 60-100 Hz — ticking on each
    /// would add that many wake-ups a second of pure cost to a loop that
    /// deliberately runs at 134 ([`App::plan_next_poll`]). `about_to_wait`
    /// runs after this and serves the deadline, so the edge still reaches the
    /// guest's mailbox before the tick that drains it.
    fn user_event(&mut self, event_loop: &ActiveEventLoop, _ev: PadWake) {
        if self.asleep {
            return;
        }
        self.next_poll = Instant::now();
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_poll));
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.asleep = false;
        let attrs = Window::default_attributes().with_title("영웅서기 5");
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                log::error!("create_window failed: {e}");
                return;
            }
        };
        let size = window.inner_size();
        self.window = Some(window);
        // Builds whichever presenter this run uses — EGL when it stands up,
        // softbuffer (Context and Surface both) otherwise.
        self.configure_surface(size.width, size.height);
        self.boot(size.width, size.height);
        // After `boot`, so the first log line of a run is the pin rather than
        // a pin nobody can place yet — and on every resume, because a trip
        // through the `background` cpuset resets the mask (see [`affinity`]).
        self.apply_affinity();
        // A finger that was down when the activity was backgrounded produced
        // no release; the guest still believes it is held.
        self.release_all_touches();
        if let Some(vm) = self.vm.as_mut() {
            if let Err(e) = vm.resume() {
                log::warn!("resume failed: {e}");
            }
        }
        self.audio_restore();
        // The surface is new; whatever it holds is not the game. Force the
        // next tick to present even though the guest has not drawn since.
        self.presented_gen = u64::MAX;
        self.last_fb.clear();
        event_loop
            .set_control_flow(ControlFlow::WaitUntil(self.next_poll.min(self.next_frame)));
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.release_all_touches();
        // Java stops dispatching once this activity is not focused, so a
        // button or stick still held would never see its release and the hero
        // would walk into a wall for the rest of the session.
        if let Some(vm) = self.vm.as_mut() {
            vm.pad_release();
        }
        if let Some(vm) = self.vm.as_mut() {
            if let Err(e) = vm.pause() {
                log::warn!("pause failed: {e}");
            }
        }
        // Silence the platform too. The guest is stopped, so it never asks for
        // this itself, and MediaPlayer would happily keep playing over
        // whatever the user switched to.
        self.audio_suspend();
        // Stop running the guest. `vm.pause()` above has already let it save
        // and stop its own timers; from here every `nativeLoop` would be work
        // nobody can see. `about_to_wait` parks on `ControlFlow::Wait` until
        // `resumed` clears this, so the process costs nothing while it is not
        // the game being played.
        self.asleep = true;
        // The surface belongs to the window that is going away. The EGL
        // presenter wraps the same dying ANativeWindow, so it goes down
        // whole too (its `Drop` releases surface, context and display);
        // `resumed` rebuilds it from scratch. `want_egl` is deliberately not
        // reset here: a run that fell back stays fallen back, rather than
        // retrying a broken driver on every unlock.
        self.egl = None;
        self.surface = None;
        self.context = None;
        self.window = None;
    }

    /// One grid, aimed at the guest's own tick ([`App::plan_next_poll`]).
    ///
    /// It began as one 16.667 ms grid, which made the display's refresh rate a
    /// ceiling on the *game speed* — `nativeLoop` advances the guest by at
    /// most one tick, so a 36 ms deadline served on 16.667 ms boundaries
    /// becomes 50 ms (ANALYSIS.md §31.9). Splitting the guest onto its own
    /// 12 ms grid was the first fix and it did not go far enough, because the
    /// device wakes this loop late. Aiming the polls instead of spacing them
    /// is the second, and it also removes the reason to keep a separate
    /// screen grid: the framebuffer changes only inside a poll.
    ///
    /// Each deadline is re-armed from `now` rather than advanced by a period,
    /// so a scene the guest cannot keep up with simply runs as fast as it can
    /// instead of accumulating a debt and then catching up in a burst.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.quit {
            event_loop.exit();
            return;
        }
        if self.asleep {
            // No window: nothing to run the guest for. `resumed` wakes the
            // loop by itself, so `Wait` here is not a deadlock.
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        let now = Instant::now();
        if now >= self.affinity_due {
            self.affinity_due = now + AFFINITY_RETRY;
            self.apply_affinity();
        }
        self.t_late += now.saturating_duration_since(self.wake_due);
        if now >= self.next_poll {
            let started = now;
            self.tick();
            let now = Instant::now();
            self.next_poll = self.plan_next_poll(started, now);
            // Presenting here rather than on a grid of its own is not a
            // shortcut, it is the accurate statement: the framebuffer can
            // only have changed inside the `tick` above, so any other moment
            // to look at it is a wake-up that finds nothing. [`FRAME`] stays
            // on as a rate limiter — it never binds while the guest repaints
            // at 27/s against a 60/s ceiling, but it is what stops a future
            // faster guest from asking the compositor for more than the
            // display can show.
            if now >= self.next_frame {
                self.next_frame = now + FRAME;
                self.present_if_new();
            }
        }
        let due = self.next_poll;
        self.wake_due = due;
        event_loop.set_control_flow(ControlFlow::WaitUntil(due));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                self.release_all_touches();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.configure_surface(size.width, size.height);
                // `boot` defers when `resumed` hands a 0x0 surface, and its
                // comment promises "the Resized that follows" completes it —
                // but nothing here ever called it back, so that path ended in
                // a VM that was never built and a black screen for the rest
                // of the session. `boot` is idempotent and re-checks the
                // size, so calling it on every resize is the whole fix.
                self.boot(size.width, size.height);
                // The resized buffer's contents are undefined until something
                // is presented into it, and the guest may not draw for
                // another tick or two — so present now, and forget the last
                // presented frame first: the content compare would otherwise
                // skip this present as "already shown", which was true of the
                // old geometry and not of the new one.
                self.last_fb.clear();
                self.present();
            }
            WindowEvent::RedrawRequested => {
                self.present();
            }
            WindowEvent::Touch(t) => {
                let (x, y) = self.to_guest(t.location.x, t.location.y);
                let release = matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled);
                let slot = self.touch_slot(t.id, x, y, release);
                let kind = match t.phase {
                    TouchPhase::Started => Touch::Pressed,
                    TouchPhase::Moved => Touch::Moved,
                    TouchPhase::Ended | TouchPhase::Cancelled => Touch::Released,
                };
                let Some(vm) = self.vm.as_mut() else { return };
                if let Err(e) = vm.touch(kind, x, y, slot) {
                    self.crashed("touch", &e);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // The hardware BACK button arrives here on Android. The guest
                // has its own meaning for it (WIPI CLEAR), and letting the
                // system consume it instead would close the activity out from
                // under a save.
                let code = match event.logical_key {
                    Key::Named(NamedKey::GoBack) | Key::Named(NamedKey::BrowserBack) => -16,
                    Key::Named(NamedKey::ArrowUp) => -1,
                    Key::Named(NamedKey::ArrowDown) => -2,
                    Key::Named(NamedKey::ArrowLeft) => -3,
                    Key::Named(NamedKey::ArrowRight) => -4,
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => -5,
                    _ => return,
                };
                let pressed = event.state == ElementState::Pressed;
                let Some(vm) = self.vm.as_mut() else { return };
                if let Err(e) = vm.key(code, pressed) {
                    self.crashed("key", &e);
                }
            }
            _ => {}
        }
    }
}

/// What the pad layer in Java has to tell the guest.
///
/// The JNI natives below run on the **Java main thread**; the VM runs on the
/// winit thread. So they may only relay — they push onto [`PAD_QUEUE`] and
/// wake the event loop, and [`App::drain_pad`] does the work on the right
/// thread. Nothing here touches `App`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PadEvent {
    Button { code: i32, down: bool, repeat: bool },
    Axes { x: f32, y: f32, hx: f32, hy: f32, lt: f32, rt: f32 },
    Present(bool),
}

/// Pad events waiting for the winit thread. A mutex and not a channel because
/// both ends are cheap and rare (a pad produces tens of events a second, and a
/// stick sample is coalesced by `hl5_core::gamepad` anyway), and because a
/// `static` needs no plumbing through `android_main` into the JNI functions.
static PAD_QUEUE: std::sync::Mutex<Vec<PadEvent>> = std::sync::Mutex::new(Vec::new());

/// Woken when a pad event arrives, so a press is served now instead of at the
/// next scheduled poll — which during gameplay can be 28 ms away
/// ([`App::plan_next_poll`] sleeps until just before the guest's next tick).
#[cfg(target_os = "android")]
static PAD_WAKE: std::sync::OnceLock<
    std::sync::Mutex<winit::event_loop::EventLoopProxy<PadWake>>,
> = std::sync::OnceLock::new();

/// The user event's payload: there is nothing to carry, the queue holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadWake;

fn push_pad(ev: PadEvent) {
    if let Ok(mut q) = PAD_QUEUE.lock() {
        // A bound, so a pad that goes berserk while the guest is stopped
        // cannot grow this without limit. Dropping the oldest is right for
        // axes (only the newest position matters) and the bound is far above
        // anything a hand can produce.
        if q.len() >= 256 {
            q.remove(0);
        }
        q.push(ev);
    }
    #[cfg(target_os = "android")]
    if let Some(proxy) = PAD_WAKE.get() {
        if let Ok(proxy) = proxy.lock() {
            // Fails only while the loop is shutting down; the queue still
            // holds the event, and nothing is going to read it.
            let _ = proxy.send_event(PadWake);
        }
    }
}

/// The answer of the last document screen the save rows opened, waiting for
/// the winit thread ([`App::save_doc_flush`]). One slot: the rows open one
/// screen at a time.
static SAVE_DOC_RESULT: std::sync::Mutex<Option<i32>> = std::sync::Mutex::new(None);

/// `Hl5Save.nativeDocResult` — any Java thread; it only relays, like the pad
/// natives below, and wakes the loop so the answer is not left until the next
/// scheduled poll.
#[cfg(target_os = "android")]
mod save_jni {
    #[no_mangle]
    pub extern "system" fn Java_kr_heroeslore_five_Hl5Save_nativeDocResult(
        _env: *mut jni::sys::JNIEnv,
        _class: jni::sys::jclass,
        status: jni::sys::jint,
    ) {
        if let Ok(mut r) = super::SAVE_DOC_RESULT.lock() {
            *r = Some(status);
        }
        if let Some(proxy) = super::PAD_WAKE.get() {
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(super::PadWake);
            }
        }
    }
}

/// The `native` methods `MainActivity` declares. Resolved by SYMBOL NAME when
/// Java calls them, so the JNI naming convention in these names is
/// load-bearing — as is `extern "system"`, the JNI calling convention.
#[cfg(target_os = "android")]
mod pad_jni {
    use super::{push_pad, PadEvent};

    #[no_mangle]
    pub extern "system" fn Java_kr_heroeslore_five_MainActivity_nativePadButton(
        _env: *mut jni::sys::JNIEnv,
        _class: jni::sys::jclass,
        code: jni::sys::jint,
        down: jni::sys::jboolean,
        repeat: jni::sys::jboolean,
    ) {
        push_pad(PadEvent::Button { code, down, repeat });
    }

    #[no_mangle]
    pub extern "system" fn Java_kr_heroeslore_five_MainActivity_nativePadAxes(
        _env: *mut jni::sys::JNIEnv,
        _class: jni::sys::jclass,
        x: jni::sys::jfloat,
        y: jni::sys::jfloat,
        hx: jni::sys::jfloat,
        hy: jni::sys::jfloat,
        lt: jni::sys::jfloat,
        rt: jni::sys::jfloat,
    ) {
        push_pad(PadEvent::Axes { x, y, hx, hy, lt, rt });
    }

    #[no_mangle]
    pub extern "system" fn Java_kr_heroeslore_five_MainActivity_nativePadPresent(
        _env: *mut jni::sys::JNIEnv,
        _class: jni::sys::jclass,
        present: jni::sys::jboolean,
    ) {
        push_pad(PadEvent::Present(present));
    }

    /// One key of the 세로 keypad (`PhoneKeypad.java`). The index is the
    /// position in `hl5_core::portrait::KEYPAD`; it travels as a pad button in
    /// a code range no Android keycode reaches, so it shares the pad's queue
    /// and wake-up but not the player's remap.
    #[no_mangle]
    pub extern "system" fn Java_kr_heroeslore_five_MainActivity_nativeKeypad(
        _env: *mut jni::sys::JNIEnv,
        _class: jni::sys::jclass,
        index: jni::sys::jint,
        down: jni::sys::jboolean,
    ) {
        if !(0..hl5_core::portrait::KEYPAD.len() as i32).contains(&index) {
            return;
        }
        push_pad(PadEvent::Button {
            code: hl5_core::portrait::KEYPAD_CODE_BASE + index,
            down,
            repeat: false,
        });
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    use winit::event_loop::EventLoop;
    use winit::platform::android::EventLoopBuilderExtAndroid;

    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_filter(
                android_logger::FilterBuilder::new().parse("warn,main=info,hl5_core=info").build(),
            )
            .with_tag("hl5"),
    );
    let Some(guest) = read_asset(&app, ASSET_GUEST) else {
        log::error!("bundled asset {ASSET_GUEST} is missing; aborting");
        return;
    };
    let Some(vfs) = read_asset(&app, ASSET_VFS) else {
        log::error!("bundled asset {ASSET_VFS} is missing; aborting");
        return;
    };
    let Some(data_dir) = app.internal_data_path() else {
        log::error!("no internal_data_path; aborting");
        return;
    };
    log::info!(
        "assets: guest {} KB, vfs {} MB; storage {data_dir:?}",
        guest.len() / 1024,
        vfs.len() / 1024 / 1024
    );

    // The guest builds absolute paths under this, the same shape as
    // /data/data/co.kr.eamobile.HeroesLore5/files on the original.
    let storage = data_dir.join("files");
    // A save the 세이브 불러오기 / 되돌리기 rows staged before the last close,
    // swapped in before anything below reads a file of the player's — the
    // pad mapping included (hl5_core::savebundle).
    for line in hl5_core::savebundle::apply_pending(&storage, hl5_core::savebundle::wall_ms()) {
        log::info!("{line}");
    }
    // `CletEntry.getAssetSize`/`loadAsset` read individual APK assets by name
    // (mtouchdevice.txt and friends). They are unpacked next to the save on
    // first run rather than served from the AssetManager, so the file layer
    // has one kind of path to think about.
    let assets = data_dir.join("assets");
    unpack_named_assets(&app, &assets);

    {
        let where_to = storage.clone();
        std::panic::set_hook(Box::new(move |info| {
            log::error!("PANIC: {info}");
            write_report(&where_to, &format!("panic: {info}\n"));
        }));
    }

    // Cloned before the app handle is moved into the event loop builder: the
    // audio bridge needs the Activity object to reach its class loader and its
    // AssetManager. `AndroidApp` is a handle, so this is a refcount.
    let sound = sound::Sound::new(&app.clone(), ASSET_VFS);
    if sound.is_none() {
        log::warn!("no audio this run; see the message above");
    }
    let savedoc = savedoc::SaveDoc::new(&app.clone());

    // NB: `EventLoop::new()` must never be called on Android — it panics with
    // "An `AndroidApp` as passed to android_main() is required". The app handle
    // has to go in through the builder.
    let event_loop = EventLoop::<PadWake>::with_user_event()
        .with_android_app(app)
        .build()
        .expect("failed to build the android event loop");
    // Publish the wake-up channel. Without this every `push_pad` finds the
    // `OnceLock` empty, `App::user_event` is dead code, and a pad edge sits in
    // the queue until the next scheduled poll — which during gameplay is
    // `plan_next_poll`'s ~28 ms away. That is the whole reason the loop is
    // built `with_user_event` at all, so it is set here, next to the builder
    // that makes it possible, and before any Java thread can be dispatching.
    if PAD_WAKE.set(std::sync::Mutex::new(event_loop.create_proxy())).is_err() {
        log::warn!("pad wake-up proxy already set; pad input will wait for the next poll");
    }

    let server = read_server(&storage);
    let account_id = read_account_id(&storage);
    let boot_mode = read_boot_mode(&storage);
    let speed = read_speed(&storage);
    let scale2x = read_scale2x(&storage);
    let screen_mode = read_resolution(&storage);
    let want_egl = read_present(&storage);
    let pin_prime = read_affinity(&storage);
    let keep_walking = read_keep_walking(&storage);
    let pad_map = read_pad_map(&storage);
    let pad_hud = read_pad_hud(&storage);
    let boost_rates = read_boost_rates(&storage);
    let dump_save = read_dump_save(&storage);
    let new_cash_items = read_new_cash_items(&storage);
    let vanilla = read_vanilla();
    let tick_floor = read_tick_cap(&storage);
    let skill_time_hz = read_skill_time(&storage, tick_floor);
    log::info!("skill time: {}", match skill_time_hz {
        0 => "guest units (text seconds = ticks / 10), no rebalance".to_string(),
        hz => format!("real seconds at {hz} ticks/s, buff rebalance on"),
    });
    let (poll, adaptive) = read_poll(&storage);
    match &server {
        Some((h, p)) => log::info!("server {h}:{p}, account {account_id}"),
        None => log::info!(
            "no server configured; answering in-process (account {account_id})"
        ),
    }
    log::info!("boot mode: {boot_mode:?}");
    log::info!("content line: {}", if vanilla { "vanilla" } else { "main" });

    let mut handler = App {
        guest,
        vfs: Arc::new(vfs),
        storage,
        assets,
        server,
        account_id,
        boot_mode,
        speed,
        keep_walking,
        boost_rates,
        dump_save,
        new_cash_items,
        skill_time_hz,
        vanilla,
        pad_map,
        pad_hud,
        window: None,
        egl: None,
        want_egl,
        pin_prime,
        affinity_note: String::new(),
        affinity_due: Instant::now(),
        context: None,
        surface: None,
        surf_w: 0,
        surf_h: 0,
        vm: None,
        dead: false,
        quit: false,
        next_frame: Instant::now(),
        next_poll: Instant::now(),
        last_tick: Instant::now(),
        presented_gen: u64::MAX,
        last_fb: Vec::new(),
        stat_skipped: 0,
        scale2x,
        screen_mode,
        // Both zero until `boot` sees a real surface; `dest_x` reads that as
        // "full width", which is the right answer before there is a picture.
        canvas_w: 0,
        full_canvas_w: 0,
        doubled: Vec::new(),
        stat_since: Instant::now(),
        stat_frames: 0,
        stat_ticks: 0,
        stat_draws: 0,
        stat_steps: 0,
        t_guest: Duration::ZERO,
        t_present: Duration::ZERO,
        t_acquire: Duration::ZERO,
        t_blit: Duration::ZERO,
        t_swap: Duration::ZERO,
        poll,
        adaptive,
        asleep: false,
        last_repaint_at: Instant::now(),
        last_repaint_count: 0,
        // The guest's design period (ANALYSIS.md §31.9) as a starting guess;
        // the first two ticks replace it with whatever this scene actually
        // does — the prologue, for one, is pinned at 55 ms.
        repaint_interval: Duration::from_millis(36),
        tick_floor,
        wake_due: Instant::now(),
        t_late: Duration::ZERO,
        stat_gl: Duration::ZERO,
        stat_flush: 0,
        touch_ids: Vec::new(),
        sound,
        savedoc,
    };
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Err(e) = event_loop.run_app(&mut handler) {
            log::error!("event loop exited with error: {e}");
        }
    }));
    if ran.is_err() {
        log::error!("the event loop panicked; closing the process rather than leaving it wedged");
    }

    // Returning from `android_main` does NOT end the process — the GameActivity
    // glue just lets this thread finish and leaves the Java Activity and the
    // whole VM alive. That is fatal: the next launch reuses the surviving
    // process and calls `android_main` again, where `EventLoop::build` refuses
    // (winit allows exactly one event loop per process), the activity dies
    // before it draws, and because the process still does not go away it
    // happens on every launch — the app becomes permanently unstartable.
    //
    // Exiting here is safe for the save: `hl5_core::files` writes through a
    // plain `std::fs::File` with no `BufWriter`, so every byte the guest wrote
    // is already in the kernel rather than in a Rust-side buffer that skipping
    // destructors would drop.
    log::info!("android_main returning; terminating the process");
    std::process::exit(0);
}

/// Where the private server's address is read from, if it is there at all.
///
/// One line, `host:port`. Read at every boot rather than baked in, so pointing
/// a phone at a different server is `adb push` and a relaunch instead of a
/// reinstall — which matters because the address is the one thing that changes
/// while everything else is being debugged.
///
/// A build-time `HL5_SERVER` is the fallback, so a build handed to a tester
/// can carry its own default. With neither, the runtime reproduces the
/// deterministic "the servers are gone" world the original now lives in.
const SERVER_FILE: &str = "hl5_server.txt";

/// This installation's identity, generated once. See [`read_account_id`].
const ACCOUNT_FILE: &str = "hl5_account.txt";

fn read_server(storage: &std::path::Path) -> Option<(String, u16)> {
    let parse = |raw: &str| -> Option<(String, u16)> {
        let (host, port) = raw.trim().rsplit_once(':')?;
        let port: u16 = port.trim().parse().ok()?;
        Some((host.trim().to_string(), port))
    };
    // The file is *parsed* before it wins, not merely found: a non-empty but
    // malformed hl5_server.txt used to short-circuit the `?` before the split
    // ever ran, silently dropping the build-time default and putting the app
    // offline with nothing in the log to say why.
    if let Ok(s) = std::fs::read_to_string(storage.join(SERVER_FILE)) {
        let s = s.trim();
        if !s.is_empty() {
            match parse(s) {
                Some(v) => return Some(v),
                None => log::warn!(
                    "{SERVER_FILE} is not host:port ({s:?}); using the build-time default"
                ),
            }
        }
    }
    option_env!("HL5_SERVER").and_then(parse)
}

/// Where the boot policy comes from: `<files>/hl5_boot.txt` if present
/// (`original` or `skip-network`, one line — adb-writable on a debug build,
/// which is what the aligned A/B on a device needs), else the build-time
/// `HL5_BOOT` (`build_android.ps1 -Boot`), else **skip-network** — "install
/// it, open it, be at the main screen" is the user requirement this app
/// ships with.
const BOOT_FILE: &str = "hl5_boot.txt";

fn read_boot_mode(storage: &std::path::Path) -> BootMode {
    let parse = |raw: &str| -> Option<BootMode> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "original" => Some(BootMode::Original),
            "skip-network" | "skipnet" | "skip" => Some(BootMode::SkipNetwork),
            _ => None,
        }
    };
    if let Ok(s) = std::fs::read_to_string(storage.join(BOOT_FILE)) {
        let s = s.trim();
        if !s.is_empty() {
            match parse(s) {
                Some(m) => return m,
                None => log::warn!("{BOOT_FILE} says {s:?}; expected `original` or `skip-network`"),
            }
        }
    }
    match option_env!("HL5_BOOT") {
        Some(s) => parse(s).unwrap_or_else(|| {
            log::warn!("HL5_BOOT={s:?} is not a boot mode; using skip-network");
            BootMode::SkipNetwork
        }),
        None => BootMode::SkipNetwork,
    }
}

/// `<files>/hl5_speed.txt` — how fast guest time runs, as a decimal.
const SPEED_FILE: &str = "hl5_speed.txt";

/// How fast to run the guest's clock. 1.0, the game as written, unless a file
/// says otherwise.
///
/// **Why the default is 1.0 and not something faster.** The guest's design tick
/// is 36 ms (27.8 a second) and on this phone gameplay was already spending
/// 7.7–8.2 s of guest CPU per 10 s wall clock *before* the blitter ports; 1.5x
/// would need 41.7 ticks a second, i.e. 1.25 s of guest CPU per second, which
/// no amount of interpreting achieves. And failing to keep up is worse than
/// not trying: `TimerCb`'s 5 ms floor means the timer is permanently already
/// due, so **the tick rate does not rise while every quantity expressed in
/// milliseconds still scales** — timed buffs burn 1.5x faster, `SaveSlotData`
/// inflates the recorded play time, and the network watchdogs shorten
/// (ANALYSIS.md §31.9).
///
/// So this is an experiment to be switched on once a scene is *measured* to
/// have headroom, not a setting to ship hot. The blitter ports of §32.6 are
/// what create that headroom; the number to look at is `repaints/s` in the
/// ten-second line, which must actually reach `27.8 x speed`.
///
/// One known inconsistency, harmless today and recorded so it is not
/// rediscovered as a bug: `stat` hands the guest a file's real `st_mtime`
/// (`files.rs`, read by `MC_fsFileAttribute` 0x14C0B8), and that value is
/// **not** scaled. A guest that compared a file time against `ftime` would see
/// them disagree; none does.
fn read_speed(storage: &std::path::Path) -> f64 {
    let parse = |raw: &str, from: &str| -> Option<f64> {
        match raw.parse::<f64>() {
            // Refuse the values that would make the clock stop or run
            // backwards rather than quietly clamping: a typo should be
            // visible in the log.
            Ok(v) if v.is_finite() && (0.1..=8.0).contains(&v) => {
                log::info!("{from}: guest clock at {v}x");
                Some(v)
            }
            _ => {
                log::warn!("{from} says {raw:?}; expected a number in 0.1..=8.0");
                None
            }
        }
    };
    // A malformed file falls through to the build-time value rather than
    // forcing 1.0 — the same rule `read_server` documents: a typo in the
    // file must not silently veto the `-Speed` a build was handed.
    if let Ok(raw) = std::fs::read_to_string(storage.join(SPEED_FILE)) {
        let raw = raw.trim();
        if !raw.is_empty() {
            if let Some(v) = parse(raw, SPEED_FILE) {
                return v;
            }
        }
    }
    // The build-time fallback exists because a release APK is not debuggable:
    // `run-as` refuses and adb cannot write `hl5_speed.txt`, so on the build
    // a tester actually measures with, `build_android.ps1 -Speed` is the only
    // way to move this lever (§33.5). The measurement contract from §32.8
    // still holds: the only honest check is `repaints/s` reaching
    // `27.8 × speed` on the device, and §33.0 says not to try above 1.0
    // until the presenter and remaining blitters have made the headroom.
    match option_env!("HL5_SPEED") {
        Some(s) => parse(s.trim(), "HL5_SPEED").unwrap_or(1.0),
        None => 1.0,
    }
}

/// `<files>/hl5_scale.txt` — the display filter: `scale2x` (default) or
/// `nearest`.
const SCALE_FILE: &str = "hl5_scale.txt";

/// Whether to run the canvas through Scale2x before the scale to the window.
///
/// On by default. It is the one picture-quality change that was measured to
/// cost almost nothing and to invent nothing — see [`App::scale2x`] — and the
/// switch exists so the difference can be looked at side by side rather than
/// argued about.
fn read_scale2x(storage: &std::path::Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(storage.join(SCALE_FILE)) else { return true };
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => true,
        "scale2x" | "on" | "1" => true,
        "nearest" | "off" | "0" => {
            log::info!("{SCALE_FILE}: nearest-neighbour display filter");
            false
        }
        other => {
            log::warn!("{SCALE_FILE} says {other:?}; expected `scale2x` or `nearest`");
            true
        }
    }
}

/// The 해상도 setting, from `<files>/hl5_resolution.txt`.
///
/// Unlike every other switch file here this one is **written by the game** —
/// the row `hl5_core::screen` draws on 환경설정 saves the player's choice into
/// the app's own files directory, which a release build can do even though it
/// is not `debuggable`. So there is no build-time escape hatch: the escape
/// hatch is the setting.
///
/// `A` (full width) for a missing or unreadable file, which is the canvas
/// every measurement in ANALYSIS.md was taken with.
fn read_resolution(storage: &std::path::Path) -> hl5_core::ScreenMode {
    let Ok(raw) = std::fs::read_to_string(storage.join(hl5_core::screen::RESOLUTION_FILE)) else {
        return hl5_core::ScreenMode::Full;
    };
    match hl5_core::ScreenMode::parse(&raw) {
        Some(m) => m,
        None => {
            log::warn!(
                "{} says {:?}; expected a, b, c or p",
                hl5_core::screen::RESOLUTION_FILE,
                raw.trim()
            );
            hl5_core::ScreenMode::Full
        }
    }
}

/// `<files>/hl5_present.txt` — how the finished canvas reaches the window:
/// `egl` (default) or `soft`.
const PRESENT_FILE: &str = "hl5_present.txt";

/// Whether to try the GPU presenter at all.
///
/// On by default: §33.0 measured the CPU present at 7.06 ms of a 36 ms
/// repaint budget, and removing it is what lets gameplay reach the guest's
/// design tick rate. The switch exists for the same reason `hl5_scale.txt`
/// does — so a difference can be looked at side by side — and because this
/// path is new: a device where EGL misbehaves subtly (rather than failing
/// outright, which falls back on its own) needs a way to be put on the old
/// path without a rebuild. On a release APK the file cannot be written
/// (not debuggable), so the build-time `HL5_PRESENT`
/// (`build_android.ps1 -Present`) is the escape hatch a shipped build
/// carries.
fn read_present(storage: &std::path::Path) -> bool {
    let parse = |raw: &str, from: &str| -> Option<bool> {
        match raw.to_ascii_lowercase().as_str() {
            "egl" | "gpu" | "on" | "1" => Some(true),
            "soft" | "softbuffer" | "cpu" | "off" | "0" => {
                log::info!("{from}: softbuffer presenter");
                Some(false)
            }
            other => {
                log::warn!("{from} says {other:?}; expected `egl` or `soft`");
                None
            }
        }
    };
    if let Ok(raw) = std::fs::read_to_string(storage.join(PRESENT_FILE)) {
        let raw = raw.trim();
        if !raw.is_empty() {
            if let Some(v) = parse(raw, PRESENT_FILE) {
                return v;
            }
        }
    }
    match option_env!("HL5_PRESENT") {
        Some(s) => parse(s.trim(), "HL5_PRESENT").unwrap_or(true),
        None => true,
    }
}

/// The poll period and whether the scheduler aims it, from the switch file
/// or the build-time default.
///
/// `auto` (the default) is the adaptive scheduler with a [`FINE`] grid;
/// `auto<N>` sets that grid to N ms; a bare `<N>` is the old fixed grid, kept
/// because it is what every number in [`App::plan_next_poll`] was measured
/// against and a claim about the scheduler has to stay checkable on the
/// device.
fn read_poll(storage: &std::path::Path) -> (Duration, bool) {
    let parse = |raw: &str, from: &str| -> Option<(Duration, bool)> {
        let raw = raw.trim().to_ascii_lowercase();
        let (adaptive, num) = match raw.strip_prefix("auto") {
            Some(rest) => (true, rest.trim()),
            None => (false, raw.as_str()),
        };
        if adaptive && num.is_empty() {
            return Some((FINE, true));
        }
        match num.parse::<u64>() {
            Ok(ms) if (1..=50).contains(&ms) => Some((Duration::from_millis(ms), adaptive)),
            _ => {
                log::warn!("{from} says {raw:?}; expected `auto`, `auto<N>` or `<N>` (1..50 ms)");
                None
            }
        }
    };
    let chosen = std::fs::read_to_string(storage.join(POLL_FILE))
        .ok()
        .and_then(|raw| parse(&raw, POLL_FILE))
        .or_else(|| option_env!("HL5_POLL").and_then(|s| parse(s, "HL5_POLL")))
        .unwrap_or((FINE, true));
    log::info!(
        "poll: {} ms, {}",
        chosen.0.as_millis(),
        if chosen.1 { "aimed at the guest tick" } else { "fixed grid" }
    );
    chosen
}

/// `<files>/hl5_rates.txt` — `on` (default) or `off`.
///
/// Controls [`hl5_core::rates`]: the rewritten `probability.dat` (orb-combine
/// success) and `smith_<N>.dat` (조합 success) the host lays down for the guest
/// to read. On by default because it is a change the user asked for, with the
/// same escape hatch every other lever here has.
///
/// **Monster drop rates are not on this switch any more.** They are the
/// game's own on every install and on both content lines — see
/// [`hl5_core::rates::restore_shipped_drop_bands`], which runs regardless of
/// this flag and deletes the widened `enemy_<N>.dat` an earlier build wrote.
///
/// **Turning it off does not undo what is left.** The override files stay
/// where they were written and `LoadRes` keeps preferring them, because the
/// guest looks at the host filesystem before the archive. Deleting
/// `<files>/data/data/co.kr.eamobile.HeroesLore5/files/c/csv/{probability,smith_*}.dat`
/// is what restores the shipped rates.
const RATES_FILE: &str = "hl5_rates.txt";

fn read_boost_rates(storage: &std::path::Path) -> bool {
    let parse = |raw: &str, from: &str| -> Option<bool> {
        match raw.to_ascii_lowercase().as_str() {
            "on" | "boost" | "1" => Some(true),
            "off" | "original" | "0" => Some(false),
            other => {
                log::warn!("{from} says {other:?}; expected `on` or `off`");
                None
            }
        }
    };
    if let Ok(raw) = std::fs::read_to_string(storage.join(RATES_FILE)) {
        let raw = raw.trim();
        if !raw.is_empty() {
            if let Some(v) = parse(raw, RATES_FILE) {
                return v;
            }
        }
    }
    match option_env!("HL5_RATES") {
        Some(s) => parse(s.trim(), "HL5_RATES").unwrap_or(true),
        None => true,
    }
}

/// `<files>/hl5_tick_cap.txt` — a tick rate in Hz, or `off`.
const TICK_CAP_FILE: &str = "hl5_tick_cap.txt";

/// The served tick-rate cap, or `None` for uncapped.
///
/// On by default at [`TICK_CAP_HZ`], because the thing it fixes is a difference
/// between *devices* and a default that varies by hardware is the bug. See
/// [`TICK_CAP_HZ`] for the measurements.
///
/// Accepts a bare number in 10..=30. Below 10 the guest's own fps table has no
/// entry that slow and the game becomes a slideshow; above 30 the cap is above
/// the binary's own 28.57 ceiling and therefore does nothing, so a value there
/// is a mistake worth reporting rather than honouring.
fn read_tick_cap(storage: &std::path::Path) -> Option<Duration> {
    let hz = |n: u32| -> Option<Duration> {
        if (10..=30).contains(&n) {
            Some(Duration::from_micros(1_000_000 / u64::from(n)))
        } else {
            log::warn!("tick cap {n} Hz is outside 10..=30; ignoring");
            None
        }
    };
    let parse = |raw: &str, from: &str| -> Option<Option<Duration>> {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("off") || raw == "0" {
            return Some(None);
        }
        match raw.parse::<u32>() {
            Ok(n) => Some(hz(n)),
            Err(_) => {
                log::warn!("{from} says {raw:?}; expected a number of Hz or `off`");
                None
            }
        }
    };
    if let Ok(raw) = std::fs::read_to_string(storage.join(TICK_CAP_FILE)) {
        if !raw.trim().is_empty() {
            if let Some(v) = parse(&raw, TICK_CAP_FILE) {
                return v;
            }
        }
    }
    match option_env!("HL5_TICK_CAP") {
        Some(sw) => parse(sw, "HL5_TICK_CAP").unwrap_or_else(|| hz(TICK_CAP_HZ)),
        None => hz(TICK_CAP_HZ),
    }
}

/// `<files>/hl5_skill_time.txt` — `on` (default) or `off`.
const SKILL_TIME_FILE: &str = "hl5_skill_time.txt";

/// The guest's own design rate (36 ms, §31.9), for when the tick cap is off.
const UNCAPPED_TICK_HZ: u32 = 28;

/// Ticks per second to arm buff durations and buff cooldowns for, so the
/// seconds the skill text prints are real — see [`hl5_core::skilltime`]; `0`
/// for the shipped behaviour (a "34초" buff over in 14.2 s at 24 Hz). The same
/// switch carries the buff rebalance (`skilltime::REBALANCE`).
///
/// On by default: the user asked for it. The rate is the served tick cap,
/// which is what every device runs at; uncapped, the guest's design rate.
/// Like the other behaviour switches it can be turned off without a rebuild.
fn read_skill_time(storage: &std::path::Path, tick_floor: Option<Duration>) -> u32 {
    let parse = |raw: &str, from: &str| -> Option<bool> {
        match raw.to_ascii_lowercase().as_str() {
            "on" | "1" => Some(true),
            "off" | "0" => Some(false),
            other => {
                log::warn!("{from} says {other:?}; expected `on` or `off`");
                None
            }
        }
    };
    let mut on = match option_env!("HL5_SKILL_TIME") {
        Some(s) => parse(s.trim(), "HL5_SKILL_TIME").unwrap_or(true),
        None => true,
    };
    if let Ok(raw) = std::fs::read_to_string(storage.join(SKILL_TIME_FILE)) {
        let raw = raw.trim();
        if !raw.is_empty() {
            if let Some(v) = parse(raw, SKILL_TIME_FILE) {
                on = v;
            }
        }
    }
    if !on {
        return 0;
    }
    match tick_floor {
        Some(d) => ((1_000_000 + d.as_micros() / 2) / d.as_micros().max(1)) as u32,
        None => UNCAPPED_TICK_HZ,
    }
}

/// `<files>/hl5_cash_items.txt` — `on` (default) or `off`.
const CASH_ITEMS_FILE: &str = "hl5_cash_items.txt";

/// Whether the cash-shop items the 2011 game never shipped are added —
/// see [`hl5_core::cashitems`].
///
/// On by default: it is content the user asked for, and with the table empty
/// the hooks decline everywhere, so there is no configuration in which the
/// items existing is wrong. It still gets a switch for the same reason
/// `keep_walking` has one — a release APK is not debuggable, and this is a new
/// behaviour change that has never run on a device, so there has to be a way
/// back that is not a rebuild.
fn read_new_cash_items(storage: &std::path::Path) -> bool {
    let parse = |raw: &str, from: &str| -> Option<bool> {
        match raw.to_ascii_lowercase().as_str() {
            "on" | "1" => Some(true),
            "off" | "0" => Some(false),
            other => {
                log::warn!("{from} says {other:?}; expected `on` or `off`");
                None
            }
        }
    };
    if let Ok(raw) = std::fs::read_to_string(storage.join(CASH_ITEMS_FILE)) {
        let raw = raw.trim();
        if !raw.is_empty() {
            if let Some(v) = parse(raw, CASH_ITEMS_FILE) {
                return v;
            }
        }
    }
    match option_env!("HL5_CASH_ITEMS") {
        Some(s) => parse(s.trim(), "HL5_CASH_ITEMS").unwrap_or(true),
        None => true,
    }
}

/// Which content line this binary is, baked in at build time.
///
/// **There is no file switch and there must not be one.** The other levers
/// here are runtime preferences; this one says which of two products the APK
/// is (`BRANCHES.md`), it is what the APK filename claims, and a build whose
/// contents could be flipped by a text file would make that filename a lie.
/// `build_android.ps1` sets `HL5_FLAVOR` from `android_app/flavor.properties`
/// (or its own `-Flavor`), so the value travels with the branch.
///
/// Anything other than `vanilla` — including absent, which is `main` — is the
/// full content line.
fn read_vanilla() -> bool {
    matches!(option_env!("HL5_FLAVOR"), Some(s) if s.trim().eq_ignore_ascii_case("vanilla"))
}

/// `<files>/hl5_dump_save.txt` — `on` or `off` (default).
const DUMP_SAVE_FILE: &str = "hl5_dump_save.txt";

/// Whether to put the player's save in the boot log — see
/// [`hl5_core::savedump`].
///
/// Off by default, and the only switch here that is off by default for a
/// reason other than replay determinism: it copies the player's save into
/// logcat. It exists because a release APK is not `debuggable`, so a bug that
/// only happens at one place on the map cannot otherwise be turned into an
/// offline replay. `build_android.ps1 -DumpSave` is the intended way in; the
/// file switch is there so the bench build can use it without a rebuild.
fn read_dump_save(storage: &std::path::Path) -> bool {
    let parse = |raw: &str, from: &str| -> Option<bool> {
        match raw.to_ascii_lowercase().as_str() {
            "on" | "dump" | "1" => Some(true),
            "off" | "0" => Some(false),
            other => {
                log::warn!("{from} says {other:?}; expected `on` or `off`");
                None
            }
        }
    };
    if let Ok(raw) = std::fs::read_to_string(storage.join(DUMP_SAVE_FILE)) {
        let raw = raw.trim();
        if !raw.is_empty() {
            if let Some(v) = parse(raw, DUMP_SAVE_FILE) {
                return v;
            }
        }
    }
    match option_env!("HL5_DUMP_SAVE") {
        Some(s) => parse(s.trim(), "HL5_DUMP_SAVE").unwrap_or(true),
        None => false,
    }
}

/// `<files>/hl5_keep_walking.txt` — `on` (default) or `off`.
const KEEP_WALKING_FILE: &str = "hl5_keep_walking.txt";

/// Whether releasing a key that is not a direction may stop the hero walking.
///
/// On by default, because the shipped behaviour is a plain bug: `HERO::KeyRelease`
/// (0x9BDCC) asks which navigation key was released and overwrites the answer
/// one instruction later, so attacking while moving ends the walk
/// (`hl5_core::input` has the whole argument). Unlike the server stand-ins
/// there is no configuration in which the original behaviour is the wanted
/// one, so this is not conditioned on anything.
///
/// It still gets a switch, for one reason: it is the only behaviour change in
/// this shell that has never run on a device, and a release APK is not
/// debuggable — without `HL5_KEEP_WALKING` (`build_android.ps1 -KeepWalking`)
/// the only way back from a misbehaving hook would be a rebuild.
fn read_keep_walking(storage: &std::path::Path) -> bool {
    let parse = |raw: &str, from: &str| -> Option<bool> {
        match raw.to_ascii_lowercase().as_str() {
            "on" | "keep" | "1" => Some(true),
            "off" | "original" | "0" => Some(false),
            other => {
                log::warn!("{from} says {other:?}; expected `on` or `off`");
                None
            }
        }
    };
    if let Ok(raw) = std::fs::read_to_string(storage.join(KEEP_WALKING_FILE)) {
        let raw = raw.trim();
        if !raw.is_empty() {
            if let Some(v) = parse(raw, KEEP_WALKING_FILE) {
                return v;
            }
        }
    }
    match option_env!("HL5_KEEP_WALKING") {
        Some(s) => parse(s.trim(), "HL5_KEEP_WALKING").unwrap_or(true),
        None => true,
    }
}

/// `<files>/hl5_gamepad.txt` — the player's pad mapping, one
/// `<android keycode>=<TOKEN>` per line. See `hl5_core::gamepad::parse_map`
/// for the tokens; anything unparseable is skipped with a warning rather than
/// being fatal, because a typo in a text file must never stop the game
/// booting.
const PAD_MAP_FILE: &str = "hl5_gamepad.txt";

/// Read the pad mapping.
///
/// The parsing itself is `hl5_core::gamepad::parse_map_reporting` and must
/// stay there: this used to be a second implementation, and the two disagreed
/// about the one thing a player can actually notice — which of two lines about
/// the same button wins. All this does is find the file and log the lines the
/// parser rejected.
fn read_pad_map(
    storage: &std::path::Path,
) -> std::collections::HashMap<i32, Option<hl5_core::gamepad::PadAction>> {
    let Ok(text) = std::fs::read_to_string(storage.join(PAD_MAP_FILE)) else {
        return std::collections::HashMap::new();
    };
    let (map, bad) = hl5_core::gamepad::parse_map_reporting(&text);
    for (n, line) in bad {
        log::warn!("{PAD_MAP_FILE}:{n}: cannot read {line:?}");
    }
    map
}

/// `<files>/hl5_pad_hud.txt` — whether each on-screen control says which pad
/// button fires it: `auto` (default: while a pad is attached), `on`, `off`.
const PAD_HUD_FILE: &str = "hl5_pad_hud.txt";

fn read_pad_hud(storage: &std::path::Path) -> hl5_core::padhud::PadHud {
    if let Ok(raw) = std::fs::read_to_string(storage.join(PAD_HUD_FILE)) {
        if !raw.trim().is_empty() {
            return hl5_core::padhud::PadHud::parse(&raw);
        }
    }
    match option_env!("HL5_PAD_HUD") {
        Some(s) => hl5_core::padhud::PadHud::parse(s),
        None => hl5_core::padhud::PadHud::default(),
    }
}

/// `<files>/hl5_affinity.txt` — which cores the guest thread may run on:
/// `prime` (default) or `off`.
const AFFINITY_FILE: &str = "hl5_affinity.txt";

/// Whether to pin the guest thread to the fastest CPU cluster.
///
/// On by default: the bench S25 never scheduled this app onto its two prime
/// cores at all, and the same loop runs 1.77x faster there ([`affinity`] has
/// the measurement). The switch exists because that is a *policy* choice
/// rather than a correctness one — it trades the scheduler's energy-optimal
/// placement for latency, so a device where it costs more battery than it
/// buys frames needs a way back without a rebuild. A release APK cannot be
/// written to (not debuggable), so `HL5_AFFINITY`
/// (`build_android.ps1 -Affinity`) is the escape hatch a shipped build
/// carries.
fn read_affinity(storage: &std::path::Path) -> bool {
    let parse = |raw: &str, from: &str| -> Option<bool> {
        match raw.to_ascii_lowercase().as_str() {
            "prime" | "fast" | "on" | "1" => Some(true),
            "off" | "none" | "any" | "0" => Some(false),
            other => {
                log::warn!("{from} says {other:?}; expected `prime` or `off`");
                None
            }
        }
    };
    if let Ok(raw) = std::fs::read_to_string(storage.join(AFFINITY_FILE)) {
        let raw = raw.trim();
        if !raw.is_empty() {
            if let Some(v) = parse(raw, AFFINITY_FILE) {
                return v;
            }
        }
    }
    match option_env!("HL5_AFFINITY") {
        Some(s) => parse(s.trim(), "HL5_AFFINITY").unwrap_or(true),
        None => true,
    }
}

/// This installation's account id, generated on first run and kept.
///
/// The guest reports it as `PHONENUMBER`, which is what the private server
/// keys accounts on (`hl5_server/protocol.py`). The original had a real phone
/// number there; a rehost has nothing equivalent, and leaving it blank would
/// put every player of every install onto one shared account — their purchase
/// histories merged, silently.
///
/// Sixteen hex characters because `PacketWrite_Login` sends the field as
/// `writeNetBuf(PHONENUMBER, 16)`. The bytes come from the OS random source
/// through `SystemTime` + the address of a stack local as a fallback — this is
/// an identifier, not a secret, so the bar is "does not collide", not
/// "unguessable".
fn read_account_id(storage: &std::path::Path) -> String {
    let path = storage.join(ACCOUNT_FILE);
    if let Ok(s) = std::fs::read_to_string(&path) {
        let s = s.trim().to_string();
        if s.len() == 16 {
            return s;
        }
    }
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15);
    seed ^= &seed as *const u64 as u64;
    let mut id = String::with_capacity(16);
    for _ in 0..16 {
        // splitmix64: enough spread for an identifier, no dependency.
        seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        id.push(char::from_digit((z % 16) as u32, 16).unwrap_or('0'));
    }
    let _ = std::fs::create_dir_all(storage);
    if let Err(e) = std::fs::write(&path, &id) {
        log::warn!("could not persist the account id: {e}");
    }
    id
}

/// Where a crash report is left for the player to fetch. Next to the save,
/// because that is the directory `adb pull`-shaped instructions already point
/// at.
const CRASH_FILE: &str = "hl5_crash.txt";

/// Cap on that file. A crash that repeats every launch would otherwise fill
/// the player's storage, and the first one is the one worth having anyway.
const CRASH_FILE_MAX: u64 = 256 * 1024;

/// Append a crash report, best effort and panic-free. Called from the panic
/// hook, so every failure here is swallowed: a panic inside a panic hook
/// aborts the process before the log line is flushed, destroying the very
/// evidence this exists to keep.
fn write_report(storage: &std::path::Path, body: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(storage);
    let path = storage.join(CRASH_FILE);
    if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > CRASH_FILE_MAX {
        return;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(f, "---- {stamp} ----\n{body}");
}

#[cfg(target_os = "android")]
fn read_asset(app: &winit::platform::android::activity::AndroidApp, name: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let manager = app.asset_manager();
    let cname = std::ffi::CString::new(name).ok()?;
    let mut asset = manager.open(&cname)?;
    let mut buf = Vec::new();
    asset.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Copy the small named assets out of the APK once, so `Config::assets` is a
/// real directory.
///
/// Only the files `CletEntry.getAssetSize`/`loadAsset` ask for by name, which
/// on this title is the touch whitelist. The 16 MB archive is *not* among them
/// — it is read by offset through `Config::vfs`, already in memory.
#[cfg(target_os = "android")]
fn unpack_named_assets(app: &winit::platform::android::activity::AndroidApp, dir: &std::path::Path) {
    const NAMED: &[&str] = &["mtouchdevice.txt"];
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    for name in NAMED {
        let path = dir.join(name);
        if path.exists() {
            continue;
        }
        match read_asset(app, name) {
            Some(bytes) => {
                if let Err(e) = std::fs::write(&path, &bytes) {
                    log::warn!("could not unpack {name}: {e}");
                }
            }
            None => log::warn!("bundled asset {name} is missing"),
        }
    }
}
