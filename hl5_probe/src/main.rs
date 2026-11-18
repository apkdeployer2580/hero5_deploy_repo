//! Headless runner. No device, no window, deterministic.
//!
//! This exists from the first day on purpose: it is the only development loop
//! this project has. Anything that reproduces here reproduces on the device,
//! because the device app links the same `hl5_core`.
//!
//! ```text
//!   cargo run -p hl5_probe -- --load-only
//!   cargo run -p hl5_probe -- --ctors
//!   cargo run -p hl5_probe -- --call MX_memInit --trace
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use hl5_core::{Config, Touch, Vm};

const USAGE: &str = "\
hl5_probe — headless bring-up runner

  --so <path>        guest binary          (default payload/libHeroesLore5.so)
  --vfs <path>       asset archive         (default payload/data.vfs.mp3)
  --storage <dir>    guest save directory  (default out/storage)
  --load-only        load and report, run nothing
  --todo             list the imports the HLE layer does not serve yet
  --selftest         drive the HLE layer through its own trampolines
  --ctors            run .init_array (the C++ static constructors)
  --boot             run the full nativeInit* boot sequence, then stop
  --frames <n>       after booting, run n nativeLoop frames
  --dump <path>      write the framebuffer as a PNG (with --boot/--frames)
  --film <dir>       write every Nth frame there as a PNG filmstrip
  --film-every <n>   filmstrip stride, in frames (default 10)
  --script <path>    drive input and dumps from a script (see below)
  --server <h:port>  dial a real game server instead of the dead-server stub
  --offline-menus    also answer the 창고접속 conversation, so that menu opens
                     and each server-only item says so in the guest's own
                     popup instead of timing out into (302). Needs
                     --local-server or --free-shop
  --boost-rates      hand the guest rewritten probability tables (hl5_core::rates)
  --keep-walking     hook HERO::KeyRelease so releasing a key that is not a
                     direction stops clearing the four movement flags — walk
                     and attack at the same time (ANALYSIS.md 37.7). Off here
                     so the 1,241-frame A/B can run without it; the shell
                     ships it on
  --local-server     answer the guest's connects in-process (localserver.rs,
                     the private server's handlers with no network at all).
                     Faithful to the deployed server, so a purchase completes
                     and grants nothing — that is --free-shop's job
  --boot-mode <m>    original (default) | skip-network: jump StateMainMenu
                     past states 0/1 right after boot — no Connecting, no
                     error popups; the intro still plays
  --account <id>     what getMin() reports; the private server keys on it
  --free-shop        cash shop without a network: implies --local-server, and
                     adds the two things that make it free — the price line is
                     struck through with 0원, and a purchase grants what the
                     game's own catalogue names for that row (kind 18,
                     index = the catalogue's `a`, count = its quantity).
                     Only with no --server; see CLAUDE.md 캐시샵을 로컬로
  --grant <spec>     ID:KIND:INDEX[:COUNT] — override the derived grant for
                     shop row ID (repeatable; implies --free-shop). Diagnostic
                     only now that the mapping is confirmed; replaces the
                     derived row rather than adding to it
  --call <symbol>    call one guest function by name, then stop
  --profile          sample the guest PC and report the hottest functions
  --profile-in <sym> also report the hottest offsets inside that function
  --profile-frames <a>..<b>
                     sample only while frames a..=b run (implies --profile).
                     A hitch is one frame; a whole-run profile averages it
                     away. Pair with --frame-cost to find the frame number
  --frame-cost <path> write one line per frame: frame number, guest
                     instructions it retired, and host wall-clock µs (which
                     includes the host ports' own time). The spikes in that
                     file are the stalls a player feels
  --trace            log every HLE call with arguments and caller
  --watch <symbol>   log every call to that guest function (repeatable)
  --count <symbol>   count entries to that guest function, changing nothing
                     about how it runs (repeatable). `MC_grpFlushLcd` -- the
                     guest's own screen repaint -- is counted on every run
  --no-patch         interpret everything; do not install the host blitters
  --screen-mode <m>  the 해상도 setting: a (FULL), b (90%), c (80%) or
                     p (세로: 240x320 and the relaid HUD; needs no --screen).
                     In a script `pad key:<label>` presses a 세로 keypad key
  --avatar           the *아바타 button and wardrobe on the character page
  --skill-time-hz <n> arm buff durations and buff cooldowns in real seconds
                     at n ticks/s (hl5_core::skilltime; the shell uses 24, its
                     tick cap). Off here so the 1,241-frame A/B can run
                     without it
  --buff-rebalance   the operator's buff rebalance (skilltime::REBALANCE):
                     x2 duration for the free 1-second buffs, x2 SP, cooldown
                     and duration for the other class buffs. The shell ships
                     it with --skill-time-hz, behind one switch
  --save-doc <path>  stand in for the phone's document screens behind the
                     세이브 내보내기/불러오기 rows (with --screen-option): an
                     export is copied to <path>, an import is read from it.
                     Without it every screen reports `cancelled`. A save the
                     rows staged is applied when the probe starts, as on the
                     phone — the summary says what happened
  --npc-menu <0..7>  after the script, open that 제련/오브/조합/도감 screen and
                     run --npc-frames frames in it, then come back
                     (repeatable). 19 KB of drawing code no replay reaches
  --npc-frames <n>   frames to spend in each of those screens (default 40)
  --screen <WxH>     run at a phone's surface size instead of the bare 400x240
                     WIPI canvas; the logical width widens to the display's
                     aspect so a uniform scale fills it (no stretch, no bars)
  --wipi-width <n>   override that logical width, to probe how wide a canvas
                     the guest actually tolerates
  --wipi-height <n>  override the logical height the same way. The shell now
                     picks one that makes the magnification a whole number
                     (2340x1080 -> 585x270, a clean 4x); this is how to try
                     another before shipping it
  --dump-audio <dir> write every clip the run registered, as its own file
  --audio-events <f> write the run's audio seam calls (Load/Play/Stop/...) in order
  -h, --help

script commands (one per line, `#` comments, run after --boot):
  frames <n>            run n nativeLoop frames
  tap <x> <y> [id]      touch pressed, a frame, touch released, in WIPI coords
  press|release|move <x> <y> [id]
                        `id` is the pointer id — which finger. Optional, 0 by
                        default, 0..127. The guest gives the touch joystick an
                        owning finger and only that one may let go of it
                        (ANALYSIS.md 37.5), so two-finger replays need it
  key <name|number>     key pressed, a frame, key released
  keydown|keyup <name|number>
                        names: up down left right select back  (WIPI codes
                        -1 -2 -3 -4 -5 -16, from WIPIConstant.getWIPIKeyCode)
  dump <path>           write the framebuffer now
  echo <text>
  learn <row> <level>   set the hero's skill/talent row to that level, the
                        byte a skill book or the 특성 screen writes
                        (`HERO+0x24c+row`). Test setup, not a game action
  belt <slot> <type> <row>
                        the guest's own `HERO::SetHotKeyBelt` on the current
                        belt page: slot 0..4 (= keys 1 3 9 * 0), type 0 skill
                        / 1 spirit / 2 item
  sp <n>                the guest's own `HERO::IncreaseSP(n)`
  skilltext <row> <file>
                        append the guest's own skill text for that row
                        (`CommonUi::GetSkillDiscription`) to <file>, EUC-KR
  skillmap              print the class's action -> skill-row table
                        (`HERO+0x2002`); `belt` takes the action number
  buffs [row]           print the hero's three buff slots, the 태세 and 정령
                        slots (type, value, ticks left / of) and, with a row,
                        that skill's cooldown (now / max ticks)
  exp-buff <ms>         call the guest's `HERO::SetExpGoodTime(ms)` -- start the
                        experience buff without buying and using the book.
                        10800001 is the super book's marker, 10800000 the
                        original's. Only meaningful once in gameplay
  npc-menu <0..7>       call the guest's own `Battle::CallNpcMenu` -- the
                        engineer/blacksmith screens. Only meaningful once the
                        replay is in gameplay
  npc-return            `BaseState::ReturnState`, back out of it
  npcbody <record>      load `npc_g.dat` record N (0 = the player, 1 = 렌) as
                        the quest window's figure through the guest's own
                        `QuestMgr::LoadNpcBody`, and print the two sheets
                        `Map::NpcPathSet` left in `_szSprFile` -- dressed,
                        the costume's number; the way a scene builds both
                        (hl5_core::npcbody). Only meaningful once in gameplay
  drop <kind> <index> [k2 v2]
                        call the guest's own `MapItem::NewDropItem` at the
                        hero's feet with the table row as-is (every override
                        -1) and, optionally, a slot-2 option pair the way a
                        drop-table band 4/6 sends one. Prints the grade and
                        the six option bytes of the item that landed, so a
                        replay can prove no displayed slot is left `<= 0`
                        (= `+랜덤옵션`). Only meaningful once in gameplay
";

struct Args {
    so: PathBuf,
    vfs: PathBuf,
    storage: PathBuf,
    /// Surface size to run at, as the phone would report it. `None` keeps the
    /// bare 400x240 WIPI canvas every replay and A/B has always used.
    screen: Option<(i32, i32)>,
    /// Logical WIPI width. Defaults to the display-matching
    /// `floor(240 * w / h)` when `--screen` is given.
    wipi_w: Option<i32>,
    /// Logical WIPI height. Defaults to [`hl5_core::WIPI_H`], or to whatever
    /// [`hl5_core::widened_canvas`] chose when `--screen` is given.
    wipi_h: Option<i32>,
    load_only: bool,
    todo: bool,
    selftest: bool,
    ctors: bool,
    boot: bool,
    frames: u32,
    dump: Option<PathBuf>,
    film: Option<PathBuf>,
    film_every: u32,
    script: Option<PathBuf>,
    server: Option<String>,
    local_server: bool,
    offline_menus: bool,
    keep_walking: bool,
    boost_rates: bool,
    /// Accept the `pad` / `pad-stick` / `pad-hud` script commands, install the
    /// field-HUD observer hook, and paint the badges. See `hl5_core::gamepad`.
    gamepad: bool,
    /// The `*아바타` button and wardrobe on the character page. See
    /// `hl5_core::avatar`.
    avatar: bool,
    /// The 해상도 setting (`hl5_core::screen`): how wide a canvas to ask
    /// `widened_canvas_for` for. Only meaningful with `--screen`.
    screen_mode: hl5_core::ScreenMode,
    /// Draw the 해상도 row on 환경설정 and let taps change it.
    screen_option: bool,
    dump_save: bool,
    new_cash_items: bool,
    /// `--skill-time-hz`: buff timers in real seconds (`hl5_core::skilltime`).
    skill_time_hz: u32,
    /// `--buff-rebalance` (`hl5_core::skilltime::REBALANCE`).
    buff_rebalance: bool,
    /// `--vanilla`: the content line without the `(진)` weapons (`BRANCHES.md`).
    vanilla: bool,
    /// `--ten-pages`: the 공용창고 at ten pages (`hl5_core::tenpages`).
    ten_pages: bool,
    /// `--save-doc`: the document the save rows' screens read and write.
    save_doc: Option<PathBuf>,
    boot_mode: hl5_core::BootMode,
    account: Option<String>,
    free_shop: bool,
    grants: Vec<String>,
    call: Option<String>,
    profile: bool,
    profile_in: Option<String>,
    /// Only sample the profile inside this inclusive frame window.
    profile_frames: Option<(u32, u32)>,
    frame_cost: Option<PathBuf>,
    trace: bool,
    watch: Vec<String>,
    count: Vec<String>,
    /// `--memwatch storage,bag`: record every PC that loads or stores inside
    /// those objects (`hl5_core::memwatch`).
    memwatch: Option<String>,
    dump_audio: Option<PathBuf>,
    /// `--audio-events <file>`: every Load/Play/Stop/Pause/Resume/Free/Volume
    /// the guest asked for, in order, as the Android shell would replay it.
    audio_events: Option<PathBuf>,
    no_patch: bool,
    /// NPC-menu screens to visit after the script, and how long to sit in
    /// each. See [`Driver::npc_menu`].
    npc_menu: Vec<i32>,
    npc_frames: u32,
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        so: PathBuf::from("payload/libHeroesLore5.so"),
        vfs: PathBuf::from("payload/data.vfs.mp3"),
        storage: PathBuf::from("out/storage"),
        screen: None,
        wipi_w: None,
        wipi_h: None,
        load_only: false,
        todo: false,
        selftest: false,
        ctors: false,
        boot: false,
        frames: 0,
        dump: None,
        film: None,
        film_every: 10,
        script: None,
        server: None,
        local_server: false,
        offline_menus: false,
        keep_walking: false,
        boost_rates: false,
        gamepad: false,
        avatar: false,
        screen_mode: hl5_core::ScreenMode::Full,
        screen_option: false,
        dump_save: false,
        new_cash_items: false,
        skill_time_hz: 0,
        buff_rebalance: false,
        vanilla: false,
        ten_pages: false,
        save_doc: None,
        boot_mode: hl5_core::BootMode::Original,
        account: None,
        free_shop: false,
        grants: Vec::new(),
        call: None,
        profile: false,
        profile_in: None,
        profile_frames: None,
        frame_cost: None,
        trace: false,
        watch: Vec::new(),
        count: Vec::new(),
        memwatch: None,
        dump_audio: None,
        audio_events: None,
        no_patch: false,
        npc_menu: Vec::new(),
        npc_frames: 40,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut need = |what: &str| it.next().ok_or_else(|| format!("{what} needs a value"));
        match arg.as_str() {
            "--screen" => {
                let v = need("--screen")?;
                let (w, h) = v
                    .split_once(['x', 'X'])
                    .ok_or_else(|| format!("--screen {v}: expected WxH"))?;
                let (w, h) = (
                    w.parse::<i32>().map_err(|_| format!("--screen {v}: bad width"))?,
                    h.parse::<i32>().map_err(|_| format!("--screen {v}: bad height"))?,
                );
                if w < 1 || h < 1 {
                    return Err(format!("--screen {v}: both extents must be positive"));
                }
                a.screen = Some((w, h));
            }
            "--screen-mode" => {
                let v = need("--screen-mode")?;
                a.screen_mode = hl5_core::ScreenMode::parse(&v)
                    .ok_or_else(|| format!("--screen-mode {v}: expected a, b, c or p"))?;
            }
            "--screen-option" => a.screen_option = true,
            "--wipi-width" => {
                a.wipi_w =
                    Some(need("--wipi-width")?.parse().map_err(|_| "--wipi-width wants a number")?);
            }
            "--wipi-height" => {
                a.wipi_h = Some(
                    need("--wipi-height")?.parse().map_err(|_| "--wipi-height wants a number")?,
                );
            }
            "--so" => a.so = PathBuf::from(need("--so")?),
            "--vfs" => a.vfs = PathBuf::from(need("--vfs")?),
            "--storage" => a.storage = PathBuf::from(need("--storage")?),
            "--load-only" => a.load_only = true,
            "--todo" => a.todo = true,
            "--selftest" => a.selftest = true,
            "--ctors" => a.ctors = true,
            "--boot" => a.boot = true,
            "--frames" => {
                a.frames = need("--frames")?.parse().map_err(|_| "--frames wants a number")?;
                a.boot = true;
            }
            "--dump" => a.dump = Some(PathBuf::from(need("--dump")?)),
            "--film" => {
                a.film = Some(PathBuf::from(need("--film")?));
                a.boot = true;
            }
            "--film-every" => {
                a.film_every =
                    need("--film-every")?.parse().map_err(|_| "--film-every wants a number")?;
            }
            "--script" => {
                a.script = Some(PathBuf::from(need("--script")?));
                a.boot = true;
            }
            "--server" => a.server = Some(need("--server")?),
            "--local-server" => a.local_server = true,
            "--offline-menus" => a.offline_menus = true,
            "--keep-walking" => a.keep_walking = true,
            // `--pad` alone shows badges as soon as a pad is reported; the
            // replay says so with `pad-hud on` or `pad connect`.
            "--pad" | "--gamepad" => a.gamepad = true,
            "--avatar" => a.avatar = true,
            "--boost-rates" => a.boost_rates = true,
            "--dump-save" => a.dump_save = true,
            "--new-cash-items" => a.new_cash_items = true,
            "--buff-rebalance" => a.buff_rebalance = true,
            "--skill-time-hz" => {
                a.skill_time_hz = need("--skill-time-hz")?
                    .parse()
                    .map_err(|_| "--skill-time-hz wants ticks per second")?;
            }
            "--vanilla" => a.vanilla = true,
            "--ten-pages" => a.ten_pages = true,
            "--save-doc" => a.save_doc = Some(PathBuf::from(need("--save-doc")?)),
            "--boot-mode" => {
                a.boot_mode = match need("--boot-mode")?.as_str() {
                    "original" => hl5_core::BootMode::Original,
                    "skip-network" | "skipnet" | "skip" => hl5_core::BootMode::SkipNetwork,
                    other => {
                        return Err(format!(
                            "--boot-mode {other}: expected `original` or `skip-network`"
                        ))
                    }
                };
            }
            "--account" => a.account = Some(need("--account")?),
            "--free-shop" => a.free_shop = true,
            "--grant" => {
                a.grants.push(need("--grant")?);
                a.free_shop = true;
            }
            "--call" => a.call = Some(need("--call")?),
            "--profile" => a.profile = true,
            "--profile-frames" => {
                let v = need("--profile-frames")?;
                let bad = || format!("--profile-frames {v}: expected <a>..<b>");
                let (lo, hi) = v.split_once("..").ok_or_else(bad)?;
                let lo: u32 = lo.parse().map_err(|_| bad())?;
                let hi: u32 = hi.parse().map_err(|_| bad())?;
                if hi < lo {
                    return Err(bad());
                }
                a.profile_frames = Some((lo, hi));
                a.profile = true;
            }
            "--frame-cost" => a.frame_cost = Some(PathBuf::from(need("--frame-cost")?)),
            "--profile-in" => {
                a.profile_in = Some(need("--profile-in")?);
                a.profile = true;
            }
            "--trace" => a.trace = true,
            "--watch" => a.watch.push(need("--watch")?),
            "--count" => a.count.push(need("--count")?),
            "--memwatch" => a.memwatch = Some(need("--memwatch")?),
            "--no-patch" => a.no_patch = true,
            "--npc-menu" => {
                let v: i32 =
                    need("--npc-menu")?.parse().map_err(|_| "--npc-menu wants a number")?;
                if !(0..=7).contains(&v) {
                    return Err(format!(
                        "--npc-menu {v}: `StateInNpcMenu::KeyPressed`'s jump table has 8 entries"
                    ));
                }
                a.npc_menu.push(v);
            }
            "--npc-frames" => {
                a.npc_frames =
                    need("--npc-frames")?.parse().map_err(|_| "--npc-frames wants a number")?;
            }
            "--dump-audio" => a.dump_audio = Some(PathBuf::from(need("--dump-audio")?)),
            "--audio-events" => a.audio_events = Some(PathBuf::from(need("--audio-events")?)),
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument `{other}`\n\n{USAGE}")),
        }
    }
    Ok(a)
}

/// `--grant ID:KIND:INDEX[:COUNT]`, the override for the mapping
/// `shop::GrantTable` otherwise derives from the game's own catalogue.
///
/// KIND is an `item_%02d.dat` table number, so 0..=18 — `ItemTable::
/// GetItemTableInfo` (0xA2728) compares it against 18 and silently does
/// nothing above that. A cash item is always 18. (An earlier version of this
/// guard rejected everything above 10, reading `readNetItem`'s short-form
/// branch as a limit on `kind`; it is not — it only means the record stops
/// after the name, which `net_item` handles.)
fn parse_grant(spec: &str) -> Result<hl5_core::shop::ShopGrant, String> {
    let bad = || format!("--grant {spec}: expected ID:KIND:INDEX[:COUNT]");
    let parts: Vec<&str> = spec.split(':').collect();
    if !(3..=4).contains(&parts.len()) {
        return Err(bad());
    }
    let id: i32 = parts[0].parse().map_err(|_| bad())?;
    let kind: u8 = parts[1].parse().map_err(|_| bad())?;
    let index: u8 = parts[2].parse().map_err(|_| bad())?;
    let count = match parts.get(3) {
        Some(c) => Some(c.parse::<u16>().map_err(|_| bad())?),
        None => None,
    };
    if kind > 18 {
        return Err(format!(
            "--grant {spec}: kind {kind} > 18 — `ItemTable::GetItemTableInfo` has 19              tables (item_00..item_18.dat) and ignores anything above, so the item              would never arrive"
        ));
    }
    Ok(hl5_core::shop::ShopGrant { id, kind, index, count })
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("\nerror: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse()?;

    let so = std::fs::read(&args.so)
        .map_err(|e| format!("{}: {e}\n(see README \"준비물\")", args.so.display()))?;
    // The archive is not needed to load or to run the constructors, so a
    // missing one is a warning here rather than a hard stop.
    let vfs = match std::fs::read(&args.vfs) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("warning: {}: {e} — continuing without the archive", args.vfs.display());
            Vec::new()
        }
    };
    std::fs::create_dir_all(&args.storage).ok();
    // A save the 세이브 불러오기/되돌리기 rows staged last run, applied before
    // anything reads the storage — the shell does the same before its own
    // reads. Nothing staged: nothing happens and nothing is printed.
    for line in hl5_core::savebundle::apply_pending(&args.storage, hl5_core::savebundle::wall_ms()) {
        println!("  {line}");
    }

    if args.trace {
        // `Vm::new` reads this; setting it here keeps the flag and the
        // environment variable from disagreeing.
        std::env::set_var("HL5_TRACE", "1");
    }

    let server = match &args.server {
        Some(s) => {
            let (h, p) = s.rsplit_once(':').ok_or("--server wants host:port")?;
            Some((h.to_string(), p.parse::<u16>().map_err(|_| "bad --server port")?))
        }
        None => None,
    };
    if (args.free_shop || args.local_server) && server.is_some() {
        // The socket layer would silently prefer the real server, which makes
        // "--grant did nothing" a puzzle instead of an error message.
        let which = if args.free_shop { "--free-shop/--grant" } else { "--local-server" };
        return Err(format!("{which} is the no-server world; drop --server"));
    }
    let shop_grants = args
        .grants
        .iter()
        .map(|s| parse_grant(s))
        .collect::<Result<Vec<_>, String>>()?;

    // The phone's geometry, reproducible headlessly. Without `--screen` this
    // is the bare WIPI canvas at magnification 1.0 — what every replay,
    // reference comparison and `patch.rs` A/B has always run, and what they
    // must keep running so their frames stay comparable.
    //
    // With it, the logical canvas widens to the display's aspect at the game's
    // design height (`floor(240 * w / h)`) so a *uniform* scale fills the
    // surface: square pixels, no letterbox, and the guest simply draws more of
    // the scene to the left and right. That is the sibling chronosSword's
    // scheme; `--wipi-width` overrides it so the guest's tolerance for a
    // non-400 canvas can be probed one width at a time.
    // The shell's formula, not a second copy of it — the probe exists to
    // reproduce what the phone does, and two implementations of "how wide is
    // the canvas" would eventually disagree about it.
    let (wipi_w, wipi_h) = match args.screen {
        Some((w, h)) => hl5_core::widened_canvas_for(w, h, args.screen_mode),
        // 세로 does not depend on a display, so it needs no `--screen`.
        None if args.screen_mode.is_portrait() => hl5_core::portrait::CANVAS,
        None => (hl5_core::WIPI_W, hl5_core::WIPI_H),
    };
    let wipi_w = args.wipi_w.unwrap_or(wipi_w);
    let wipi_h = args.wipi_h.unwrap_or(wipi_h);
    if wipi_w < 1 || wipi_h < 1 {
        return Err(format!("canvas {wipi_w}x{wipi_h}: both extents must be positive"));
    }

    let cfg = Config {
        // Scheme B, as on the device: the guest's screen *is* the canvas, so
        // magnification is 1.0 and the framebuffer comes out canvas-sized —
        // dumps stay small and comparable, and the host does the display
        // upscale exactly once, in the shell.
        screen_w: wipi_w,
        screen_h: wipi_h,
        wipi_w,
        wipi_h,
        vfs: Arc::new(vfs),
        storage: args.storage.clone(),
        assets: args.so.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
        server,
        local_server: args.local_server,
        offline_menus: args.offline_menus,
        keep_walking: args.keep_walking,
        boost_rates: args.boost_rates,
        gamepad: args.gamepad,
        avatar: args.avatar,
        screen_mode: args.screen_mode,
        screen_option: args.screen_option,
        scale2x: true,
        dump_save: args.dump_save,
        new_cash_items: args.new_cash_items,
        skill_time_hz: args.skill_time_hz,
        buff_rebalance: args.buff_rebalance,
        vanilla: args.vanilla,
        ten_pages: args.ten_pages,
        account_id: args.account.clone().unwrap_or_default(),
        boot_mode: args.boot_mode,
        free_shop: args.free_shop,
        shop_grants,
        ..Config::default()
    };

    let t0 = std::time::Instant::now();
    let mut vm = Vm::new(&so, cfg)?;
    let load_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // ---- what the loader produced ----------------------------------------
    println!("== loaded {} ==", args.so.display());
    println!("  image        {:#010x}..{:#010x}  ({} KB)",
             vm.elf.base, vm.elf.image_end, (vm.elf.image_end - vm.elf.base) / 1024);
    println!("  symbols      {} defined, {} with a declared size",
             vm.elf.symbols.len(), vm.elf.sizes.len());
    println!("  imports      {} bound to trampolines", vm.imports.len());
    println!("  exidx        {:#010x} + {} bytes ({} entries)",
             vm.elf.exidx.0, vm.elf.exidx.1, vm.elf.exidx.1 / 8);
    for (label, (addr, count)) in
        [(".init_array", vm.elf.init_array), (".fini_array", vm.elf.fini_array)]
    {
        if count == 0 {
            println!("  {label:<12} absent");
            continue;
        }
        let entries: Vec<String> = (0..count)
            .map(|i| {
                let f = vm.mem.read_u32(addr + i * 4);
                format!("{:#010x} {}", f, vm.describe(f))
            })
            .collect();
        println!("  {label:<12} {addr:#010x} x{count} -> {}", entries.join(", "));
    }
    println!("  load took    {load_ms:.1} ms");

    // A few landmarks, to prove symbols resolve to sane places.
    println!("\n== landmarks ==");
    for name in [
        "Java_co_kr_eamobile_CletEntry_nativeInitKernel",
        "Java_co_kr_eamobile_CletEntry_nativeLoop",
        "MC_grpGetDisplayInfo",
        "_midas_funcFntDrawString",
        "startDes",
        "_ZN7Graphic24DrawImageBlendingClip565EiiiiiP8GbmImageiihhhi",
    ] {
        match vm.symbol(name) {
            Some(a) => println!("  {a:#010x}  {name}"),
            None => println!("  {:<10}  {name}   *** MISSING ***", "-"),
        }
    }

    if args.todo {
        let done: std::collections::HashSet<&str> =
            hl5_core::hle::IMPLEMENTED.iter().copied().collect();
        let mut left: Vec<&String> =
            vm.imports.iter().filter(|n| !done.contains(n.as_str())).collect();
        left.sort();
        println!(
            "\n== HLE layer: {} of {} imports served, {} to go ==",
            vm.imports.len() - left.len(),
            vm.imports.len(),
            left.len()
        );
        let mut by_group: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
        for n in &left {
            by_group.entry(hl5_core::hle::group(n)).or_default().push(n.as_str());
        }
        for (g, names) in by_group {
            println!("  {g:<9} ({:>2})  {}", names.len(), names.join(", "));
        }
    }

    if args.selftest {
        return selftest(&mut vm);
    }

    if args.load_only || args.todo {
        return Ok(());
    }

    // A frame window starts sampling from the driver instead, at its first
    // frame, so everything before it — boot, the walk to the scene — is not
    // in the report at all.
    if args.profile && args.profile_frames.is_none() {
        vm.profile = Some(Default::default());
    }

    // The reference half of the A/B: `--no-patch` interprets every guest
    // function, which is what a host port has to reproduce byte for byte.
    if !args.no_patch {
        let on = vm.install_patches()?;
        println!("  host patches {}", on.join(", "));
    }

    for sym in &args.watch {
        if !vm.watch_symbol(sym) {
            return Err(format!("--watch {sym}: no such guest symbol"));
        }
    }

    // `MC_grpFlushLcd` -- the guest's own repaint -- is not listed here:
    // `Vm::boot` counts it for every run, because `hud` needs the same fact
    // (see `GUEST_REPAINT` in hl5_core).
    for sym in &args.count {
        if !vm.count_symbol(sym) {
            return Err(format!("--count {sym}: no such guest symbol"));
        }
    }

    if let Some(kinds) = &args.memwatch {
        for l in hl5_core::memwatch::arm(&mut vm, kinds, args.ten_pages)? {
            println!("  memwatch {l}");
        }
    }

    // ---- run something ----------------------------------------------------
    let outcome = if args.boot {
        println!("\n== boot ==");
        if args.boot_mode == hl5_core::BootMode::SkipNetwork {
            println!("  boot mode: skip-network");
        }
        let mut d = Driver::new(&args)?;
        let mut r = vm.boot();
        if r.is_ok() {
            r = d.frames(&mut vm, args.frames);
        }
        if r.is_ok() {
            if let Some(path) = &args.script {
                let text = std::fs::read_to_string(path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                r = d.run_script(&mut vm, &text);
            }
        }
        // Sugar for the two script commands, so a screen can be looked at
        // without editing a replay file. A scene meant for the A/B set should
        // be a script, not this.
        for state in &args.npc_menu {
            if r.is_err() {
                break;
            }
            println!("  [{}] npc-menu {state}", d.n);
            r = d
                .npc_menu(&mut vm, *state)
                .and_then(|()| d.frames(&mut vm, args.npc_frames))
                .and_then(|()| d.npc_return(&mut vm))
                .and_then(|()| d.frames(&mut vm, 2));
        }
        if r.is_ok() {
            if let Some(path) = &args.dump {
                dump_png(&vm, path)?;
                println!("  wrote {} ({}x{})", path.display(), vm.gl.fb_w, vm.gl.fb_h);
            }
        }
        d.finish(&mut vm)?;
        r.map(|()| 0u32)
    } else if args.ctors {
        println!("\n== running .init_array ==");
        vm.run_init_array().map(|()| 0u32)
    } else if let Some(sym) = &args.call {
        let addr = vm.symbol(sym).ok_or_else(|| format!("no such guest symbol: {sym}"))?;
        // JNI entry points take (JNIEnv*, jclass, ...) — hand them the
        // synthetic env so they get as far as their first real JNI call.
        let call_args: &[u32] =
            if sym.starts_with("Java_") { &[vm.jni_env(), 0] } else { &[] };
        println!("\n== calling {sym} @ {addr:#010x} ==");
        vm.call(addr, call_args)
    } else {
        println!("\n(nothing to run — pass --ctors, --call <symbol>, or --load-only)");
        return Ok(());
    };

    let t1 = std::time::Instant::now();
    let _ = t1;

    if let Some(dir) = &args.dump_audio {
        dump_audio(&vm, dir)?;
    }
    if let Some(path) = &args.audio_events {
        // Headless nothing drains the queue, so this is the whole run's seam
        // history — the same sequence `hl5_android::sound` hands `Hl5Audio`.
        let events = vm.audio.take_events();
        let text: String = events.iter().map(|e| format!("{e:?}
")).collect();
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("  audio events        {} written to {}", events.len(), path.display());
    }

    for line in &vm.log {
        println!("  {line}");
    }

    if !vm.jni.names().is_empty() {
        println!("\n== JNI lookups (in order) ==");
        for n in vm.jni.names() {
            println!("  {n}");
        }
    }

    println!("\n== result ==");
    println!("  guest instructions  {}", vm.steps);
    println!("  HLE calls served    {}", vm.hle_calls);
    if vm.patch_calls > 0 || vm.patch_declined > 0 {
        println!("  host patch calls    {} ({} declined)", vm.patch_calls, vm.patch_declined);
    }
    if vm.des.served > 0 {
        // Every save, save-slot read and encrypted resource goes through
        // `startDes`; zero here with saves in the replay means the self-check
        // did not pass and the guest kept the job (see the `des:` log lines).
        println!("  host DES            {} startDes call(s) served", vm.des.served);
    }
    if args.free_shop {
        // Zero here means the shop price was never drawn — the replay did not
        // reach the cash shop, or the hook's call-site key no longer matches.
        println!("  free-shop hook      {} price line(s) redrawn", vm.hook_calls);
    }
    if args.keep_walking && !args.free_shop {
        // `Vm::hook_calls` is one counter shared by every hook, so this is
        // only attributable when `--keep-walking` is the only one installed —
        // hence the `!free_shop`. Zero means the replay never released a key
        // that was not a direction while a hero existed, which is a statement
        // about the replay and not about the hook. `--count 0x9be74` is the
        // sharper question: how often the shipped code would have planted the
        // standing animation on a walking hero.
        println!("  keep-walking hook   {} non-direction release(s) suppressed", vm.hook_calls);
    }
    if args.gamepad {
        // Four numbers, because each answers a different failure. `edges`
        // zero means the replay never pressed anything; `unbound` counts
        // presses the map has no action for; `delivered` is what actually
        // reached the guest (it lags `edges` by whatever is still queued
        // behind a full mailbox); `badges` zero with a HUD on screen means the
        // rects were found but nothing is bound to them.
        println!(
            "  gamepad             {} edge(s), {} unbound, {} event(s) delivered, {} queued, {} drag(s) merged into a press, {} poll(s) held back for order",
            vm.pad.edges,
            vm.pad.unbound,
            vm.pad.delivered,
            vm.pad.queued(),
            vm.pad.merged,
            vm.pad.held_back
        );
        println!(
            "  pad badges          {} painted, HUD {} ({}), field {}",
            vm.pad.badges,
            vm.pad.hud.as_token(),
            if vm.pad.badges_visible() { "showing" } else { "hidden" },
            if vm.pad.field() { "up" } else { "not drawn" }
        );
        if let Some(ui) = vm.pad.ui {
            println!(
                "  field HUD rects     buttons {:?} page {:?} ok {:?}",
                ui.buttons, ui.page, ui.ok
            );
        }
    }
    if args.avatar {
        // Buttons drawn but no page means the replay never opened it; pages
        // drawn but no dressing means the substitution never ran.
        println!(
            "  avatar              {} button(s), {} page(s) drawn, opened {}x, {} change(s), {} layer(s) dressed, {} rig cif read(s), {} row(s) scrolled, {} scene body(ies) dressed",
            vm.avatar.buttons_drawn,
            vm.avatar.pages_drawn,
            vm.avatar.opened,
            vm.avatar.changes,
            vm.avatar.dressed,
            vm.avatar.cif_swaps,
            vm.avatar.scrolled,
            vm.avatar.bodies_dressed
        );
        // The spirit's wardrobe: a button with no opening means the tap missed
        // it; changes with no re-dress means `ChangeSprImg` never ran; `dressed`
        // counts her sheets rewritten on their way into `LoadSprImg`.
        let s = vm.avatar.spirit_counters();
        println!(
            "  avatar (spirit)     {} button(s), opened {}x, {} re-dress(es), {} load(s) dressed, {} scene body(ies) dressed, {} outfit(s)",
            s.0, s.1, s.2, s.3, s.4,
            vm.avatar.spirit_looks.len()
        );
    }
    if args.screen_option {
        // Its own counters, not `Vm::hook_calls` — that one is shared by every
        // hook, and the whole point of this line is "did the row appear, and
        // did a tap reach it". Zero rows means the replay never opened
        // 환경설정; zero taps with rows drawn means the cells are not where
        // the script thinks they are.
        println!(
            "  resolution row      {} draw(s), {} tap(s), {} restart prompt(s); \
             now {} ({}){}",
            vm.screen.drawn,
            vm.screen.taps,
            vm.screen.asked,
            vm.screen.chosen.letter(),
            vm.screen.chosen.label(),
            if vm.screen.needs_restart() { ", restart to apply" } else { "" }
        );
        // 배치변경: the editor's counters, the field hooks' counters (a belt
        // or touch-UI redraw counts one frame drawn where the layout put it),
        // and the fingers translated / swallowed.
        println!(
            "  배치변경            {}{}{}, editor opened {}x, {} move(s), {} resize(s), {} save(s); field belt {}x, touch UI {}x redrawn ({} capture(s) reused, {} with a registration mark); {} menu belt touch(es) laid out; {} finger(s) routed, {} swallowed",
            if vm.hudlayout.installed { "" } else { "not on this canvas, " },
            if vm.hudlayout.custom { "커스텀" } else { "기본" },
            if vm.hudlayout.edit.active { " (editor up)" } else { "" },
            vm.hudlayout.edit.opened,
            vm.hudlayout.edit.moved,
            vm.hudlayout.edit.resized,
            vm.hudlayout.saved,
            vm.hudlayout.belts,
            vm.hudlayout.touch_uis,
            vm.hudlayout.reused,
            vm.hudlayout.marked,
            vm.hudlayout.menu_slots,
            vm.hudlayout.routed,
            vm.hudlayout.swallowed,
        );
        println!("  HUD images          {}", hl5_core::hudlayout::dims_report(&vm));
        println!(
            "  option rows         {} host row draw(s); 화면필터 {}; 패드키 {}{}, mapping screen opened {}x, {} binding(s) made, {} override(s)",
            vm.options.drawn,
            if vm.scale2x { "ON" } else { "OFF" },
            if vm.padmap.custom { "커스텀" } else { "기본" },
            if vm.padmap.active { " (screen up)" } else { "" },
            vm.padmap.opened,
            vm.padmap.assigned,
            vm.pad.overrides.len(),
        );
        println!(
            "  save rows           {} export(s), {} pick(s), {} staged, {} question(s), {} message(s), {} game save(s) first; vault {} generation(s){}",
            vm.savexfer.exports,
            vm.savexfer.picks,
            vm.savexfer.staged,
            vm.savexfer.questions,
            vm.savexfer.messages,
            vm.savexfer.saved_first,
            hl5_core::savebundle::vault_list(&args.storage).len(),
            if args.storage.join(hl5_core::savebundle::PENDING).exists() { ", a save staged for the next start" } else { "" }
        );
        // Whether the guest actually asked to be closed. On a phone that is
        // the window going away; here it is the one bit that says 예 was
        // answered and reached `MC_knlExit`, which is otherwise invisible
        // because the probe has no window to close.
        if vm.exit_requested {
            println!("  guest exit          asked for (MC_knlExit -> CletEntry.exitApp, or a save row's 예)");
        }
    }
    if args.ten_pages {
        println!(
            "  ten pages           {} chest(s) moved, {} cell table(s) moved, {} tab tap(s), {} column key(s); \
             bag tail {} saved, {} loaded",
            vm.pages10.chests,
            vm.pages10.cell_tables,
            vm.pages10.taps,
            vm.pages10.keys,
            vm.pages10.tails_saved,
            vm.pages10.tails_loaded
        );
    }
    if !vm.audio.is_empty() {
        let playing = vm.audio.iter().filter(|(_, c)| c.plays > 0).count();
        println!("  audio clips         {} registered, {playing} played", vm.audio.len());
    }
    println!("  heap live / peak    {} / {} bytes", vm.heap.live_bytes, vm.heap.peak_bytes);
    // The host's own upscale pass, and the guest's own repaint count. Neither
    // is visible in `guest instructions`: the first is host time and the
    // second is a *rate* question — how many of the frames run were frames
    // the guest actually redrew. See `GlState::draw_time` and `Vm::probes`.
    println!(
        "  gl draw pass        {} calls, {:.0} ms",
        vm.gl.draws,
        vm.gl.draw_time.as_secs_f64() * 1000.0
    );
    for (name, n) in vm.probe_counts() {
        println!("  counted             {n:>8}  {name}");
    }

    if args.memwatch.is_some() {
        println!("\n== memwatch ==");
        for l in hl5_core::memwatch::report(&vm) {
            println!("  {l}");
        }
    }

    if args.profile {
        println!("\n== hottest guest functions ==");
        for (name, n, pct) in vm.profile_report(15) {
            println!("  {pct:5.1}%  {n:>8}  {name}");
        }
        if let Some(f) = &args.profile_in {
            println!("
== hottest offsets inside {f} ==");
            for (off, n, pct) in vm.profile_inside(f, 0x40, 12) {
                println!("  {pct:5.1}%  {n:>8}  {off}");
            }
        }
    }

    match outcome {
        Ok(r) => {
            println!("  returned            {r} ({r:#x})");
            println!("\nOK");
            Ok(())
        }
        Err(e) => Err(e),
    }
}

// ---- frame driver: filmstrip + input script --------------------------------
//
// The guest is event-driven: `nativeLoop` only runs `timerLoop` +
// `updateScreen`, and the game's own 50 ms tick timer (armed at the end of
// `onStartApp`) is what actually advances it. So "run the game" means "call
// frame() repeatedly", and anything worth looking at needs the frame counter
// shared between the `--frames` prologue, the script and the filmstrip —
// hence one driver rather than three loops.

struct Driver {
    /// Frames run so far, across `--frames` and the script alike. This is the
    /// number in the filmstrip file names, so a frame is findable from a log.
    n: u32,
    film: Option<PathBuf>,
    every: u32,
    /// `--profile-frames`: sample only while these frames (1-based, the same
    /// numbers as the filmstrip) run.
    profile_window: Option<(u32, u32)>,
    /// The window's samples once it has closed, handed back to the `Vm` by
    /// [`Driver::finish`] so the ordinary report prints them.
    profile_done: Option<std::collections::HashMap<u32, u64>>,
    /// `--frame-cost`: one line per frame.
    cost: Option<std::io::BufWriter<std::fs::File>>,
    /// The costliest frame seen, for the summary line: (frame, instructions,
    /// µs). A replay's stall is usually one frame, and this names it.
    worst: (u32, u64, u128),
    /// `--save-doc`: what stands in for the phone's document screens.
    save_doc: Option<PathBuf>,
}

/// Virtual milliseconds per frame. The device runs `onDrawFrame` at the
/// display rate; 33 keeps the clock at a plausible 30 fps so the guest's
/// timers fire at the rate they were written for.
const FRAME_MS: i64 = 33;

impl Driver {
    fn new(args: &Args) -> Result<Self, String> {
        let cost = match &args.frame_cost {
            Some(path) => {
                if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                let f = std::fs::File::create(path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                Some(std::io::BufWriter::new(f))
            }
            None => None,
        };
        Ok(Driver {
            n: 0,
            film: args.film.clone(),
            every: args.film_every.max(1),
            profile_window: args.profile_frames,
            profile_done: None,
            cost,
            worst: (0, 0, 0),
            save_doc: args.save_doc.clone(),
        })
    }

    /// Answer a save row's document request the way `Hl5Save.java` does: the
    /// export copied to the document, or the document copied in — with the
    /// shell's own cap — or `cancelled` when there is no document to use.
    fn serve_save_doc(&self, vm: &mut Vm) {
        use hl5_core::savexfer::{DocRequest, DocStatus};
        let Some(req) = vm.take_save_doc_request() else { return };
        let status = match (&req, &self.save_doc) {
            (_, None) => DocStatus::Cancelled,
            (DocRequest::Export { file, name }, Some(doc)) => {
                let r = std::fs::copy(file, doc).and_then(|_| std::fs::remove_file(file));
                println!("  [{}] save-doc: export ({name}) -> {}: {r:?}", self.n, doc.display());
                if r.is_ok() { DocStatus::Done } else { DocStatus::Failed }
            }
            (DocRequest::Import { file }, Some(doc)) => {
                let r = std::fs::read(doc).and_then(|mut b| {
                    b.truncate(hl5_core::savebundle::BUNDLE_MAX + 1);
                    std::fs::write(file, b)
                });
                println!("  [{}] save-doc: import <- {}: {r:?}", self.n, doc.display());
                if r.is_ok() { DocStatus::Done } else { DocStatus::Failed }
            }
        };
        vm.save_doc_done(status);
    }

    /// Close the books: flush `--frame-cost`, name the costliest frame, and
    /// hand a closed `--profile-frames` window back to the `Vm` for the report.
    fn finish(&mut self, vm: &mut Vm) -> Result<(), String> {
        if let Some(w) = self.cost.as_mut() {
            use std::io::Write;
            w.flush().map_err(|e| format!("--frame-cost: {e}"))?;
            let (f, steps, us) = self.worst;
            println!("  costliest frame     {f}: {steps} guest instructions, {:.1} ms", us as f64 / 1000.0);
        }
        if let Some(p) = self.profile_done.take() {
            vm.profile = Some(p);
        } else if let Some((lo, _)) = self.profile_window {
            if self.n < lo {
                println!("  --profile-frames    the run ended at frame {} before the window opened", self.n);
            }
        }
        Ok(())
    }

    fn frames(&mut self, vm: &mut Vm, n: u32) -> Result<(), String> {
        for _ in 0..n {
            let k = self.n + 1;
            if self.profile_window.is_some_and(|(lo, _)| lo == k) {
                vm.profile = Some(Default::default());
            }
            let (steps0, t0) = (vm.steps, std::time::Instant::now());
            vm.frame(FRAME_MS).map_err(|e| format!("frame {}: {e}", self.n))?;
            let (steps, us) = (vm.steps - steps0, t0.elapsed().as_micros());
            self.n += 1;
            self.serve_save_doc(vm);
            if self.profile_window.is_some_and(|(_, hi)| hi == k) {
                self.profile_done = vm.profile.take();
            }
            if let Some(w) = self.cost.as_mut() {
                use std::io::Write;
                writeln!(w, "{k} {steps} {us}").map_err(|e| format!("--frame-cost: {e}"))?;
                if steps > self.worst.1 {
                    self.worst = (k, steps, us);
                }
            }
            if let Some(dir) = &self.film {
                if self.n % self.every == 0 {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| format!("{}: {e}", dir.display()))?;
                    dump_png(vm, &dir.join(format!("f{:05}.png", self.n)))?;
                }
            }
        }
        Ok(())
    }

    /// Open one of `StateInNpcMenu`'s eight screens the way the game does.
    ///
    /// The four replay scenes the `patch.rs` A/B is built from never reach
    /// `StateInNpcMenu`: `Battle::CallNpcMenu` is called zero times across all
    /// 905 frames. That leaves `DrawRefine` (4,820 B) + `DrawOrbCombine`
    /// (4,664 B) + `DrawNormalMix` (2,996 B) + `DrawCollection` (6,592 B) —
    /// **19 KB of drawing code outside the acceptance test**, which a blitter
    /// port could quietly corrupt with nobody the wiser (ANALYSIS.md §31.6).
    ///
    /// This is the *same call* `EventProc::Event_UiEngineer` makes at 0x70058:
    /// `mov r1, #4` then `bl Battle::CallNpcMenu`. Both that function (0x45A28)
    /// and `BaseState::ReturnState` (0x44E20) ignore `this` entirely — each
    /// loads the live state out of `gv+0x27C` for itself and goes through its
    /// vtable (slot 0x80 and slot 4) — so a null `this` is not a shortcut, it
    /// is the argument being dead.
    ///
    /// State numbering (`StateInNpcMenu::KeyPressed`'s 8-entry jump table, and
    /// mind the `addls pc, pc, rX, lsl #2` idiom: **case 0 is `pc+8`**, the
    /// slot *after* the fall-through):
    /// 2 판매, 3 일반제련 (`LowRefine` — can crack and destroy), 4 고급제련
    /// (`HighRefine`), 5 오브결합, 6 조합, 7 도감.
    fn npc_menu(&mut self, vm: &mut Vm, state: i32) -> Result<(), String> {
        const CALL: &str = "_ZN6Battle11CallNpcMenuEas";
        let f = vm.symbol(CALL).ok_or_else(|| format!("no `{CALL}` in this binary"))?;
        vm.call(f, &[0, state as u32, 0]).map(|_| ())
    }

    /// Start the experience buff, so a replay can reach the code the
    /// 슈퍼 성장의 서 exists to exercise.
    ///
    /// Diagnostics, and it is here rather than in `hl5_core` for a reason: the
    /// only other way to activate the buff offline is to buy the book in the
    /// cash shop and use it out of the bag, which is a long stretch of fragile
    /// menu navigation. This calls the guest's own
    /// `HERO::SetExpGoodTime(long long)` — the same function the item's own
    /// path calls — so what it sets up is indistinguishable from the real
    /// thing. Pass 10800001 for the super book's marker duration and 10800000
    /// for the original's.
    ///
    /// `this` is `gv[0x1474]`, and the `long long` lands in r2/r3 because
    /// AAPCS pads r1 for an even-aligned pair.
    /// The live `HERO*`, or an error naming the command that needed it.
    fn hero(vm: &Vm, cmd: &str) -> Result<u32, String> {
        const GV_HERO: u32 = 0x1474;
        let gv = vm.symbol("gv").ok_or("no `gv` in this binary")?;
        match vm.mem.read_u32(gv + GV_HERO) {
            0 => Err(format!("{cmd}: no HERO yet — the replay is not in gameplay")),
            hero => Ok(hero),
        }
    }

    /// `learn <row> <level>`: the skill-level byte `HERO+0x24c+row`, which the
    /// skill books (`StateInNpcMenu::Process+0x300`, `StateInGameMenu::
    /// Process+0x11e8`) and the 특성 screen write and `UseSkill` copies into
    /// `HeroSkillInfo+0x44`. A replay cannot reach a talent tree.
    fn learn(&mut self, vm: &mut Vm, row: i32, level: i32) -> Result<(), String> {
        const LEVELS: u32 = 0x24c;
        if !(0..=58).contains(&row) || !(0..=7).contains(&level) {
            return Err(format!("learn: row {row} level {level} out of range"));
        }
        let hero = Self::hero(vm, "learn")?;
        vm.mem.write_bytes(hero + LEVELS + row as u32, &[level as u8]);
        Ok(())
    }

    /// `belt <slot> <type> <row>`: `HERO::SetHotKeyBelt(slot, page, type, row)`
    /// (0x88b9c) on the page `HERO::GetHotKeyToggle` says is showing.
    fn belt(&mut self, vm: &mut Vm, slot: i32, kind: i32, row: i32) -> Result<(), String> {
        const SET: &str = "_ZN4HERO13SetHotKeyBeltEaaaa";
        const PAGE: &str = "_ZN4HERO15GetHotKeyToggleEv";
        let hero = Self::hero(vm, "belt")?;
        let set = vm.symbol(SET).ok_or_else(|| format!("no `{SET}` in this binary"))?;
        let page = vm.symbol(PAGE).ok_or_else(|| format!("no `{PAGE}` in this binary"))?;
        let page = vm.call(page, &[hero])?;
        vm.call(set, &[hero, slot as u32, page, kind as u32, row as u32]).map(|_| ())
    }

    /// `buffs [row]`: the hero's timers as the guest holds them, in ticks.
    /// Slots from `BATTLER::AddBuffSkill` / `HERO::AddStanceSkill` /
    /// `HERO::AddSpiritSkill`; the cooldown from `HeroSkillInfo+0x54/+0x56`.
    fn buffs(&mut self, vm: &mut Vm, row: Option<i32>) {
        let Ok(hero) = Self::hero(vm, "buffs") else {
            println!("  [{}] buffs: not in gameplay", self.n);
            return;
        };
        let i16_at = |a: u32| vm.mem.read_u16(a) as i16;
        let mut line = String::new();
        for k in 0..3 {
            line += &format!(
                " buff{k}={}:{} {}/{}",
                vm.mem.read_u8(hero + 0x11b + k) as i8,
                i16_at(hero + 0x11e + 2 * k),
                i16_at(hero + 0x124 + 2 * k),
                i16_at(hero + 0x12a + 2 * k)
            );
        }
        for (name, ty, val, cur, max) in
            [("stance", 0x285, 0x286, 0x288, 0x28a), ("spirit", 0x28c, 0x28e, 0x290, 0x292)]
        {
            line += &format!(
                " {name}={}:{} {}/{}",
                vm.mem.read_u8(hero + ty) as i8,
                i16_at(hero + val),
                i16_at(hero + cur),
                i16_at(hero + max)
            );
        }
        line += &format!(" sp={}", i16_at(hero + 0x248));
        if let Some(row) = row {
            let info = hero + 0x348 + 0x58 * row as u32;
            line += &format!(" cool[{row}]={}/{}", i16_at(info + 0x54), i16_at(info + 0x56));
        }
        println!("  [{}] buffs:{line}", self.n);
        vm.log.push(format!("[{}] buffs:{line}", self.n));
    }

    /// `sp <n>`: the guest's own `HERO::IncreaseSP(n)` (0x88e2c), which clamps
    /// to the hero's maximum. A new 워리어 starts with none.
    fn sp(&mut self, vm: &mut Vm, n: i32) -> Result<(), String> {
        const INC: &str = "_ZN4HERO10IncreaseSPEi";
        let hero = Self::hero(vm, "sp")?;
        let f = vm.symbol(INC).ok_or_else(|| format!("no `{INC}` in this binary"))?;
        vm.call(f, &[hero, n as u32]).map(|_| ())
    }

    /// `skilltext <row> <file>`: the guest's own skill text for that row —
    /// `CommonUi::GetSkillDiscription(this, buf, row, NULL)` (0x7a7b4), which
    /// looks the info up itself and never reads `this` — appended to `file`
    /// as one EUC-KR line (`;` is the game's line break).
    fn skilltext(&mut self, vm: &mut Vm, row: i32, file: &Path) -> Result<(), String> {
        const TEXT: &str = "_ZN8CommonUi19GetSkillDiscriptionEPcaP13HeroSkillInfo";
        Self::hero(vm, "skilltext")?;
        let f = vm.symbol(TEXT).ok_or_else(|| format!("no `{TEXT}` in this binary"))?;
        let buf = vm.heap.alloc(1024).ok_or("skilltext: guest heap is full")?;
        vm.mem.write_bytes(buf, &[0u8; 1024]);
        vm.call(f, &[0, buf, row as u32, 0])?;
        let mut text = Vec::new();
        for i in 0..1024 {
            match vm.mem.read_u8(buf + i) {
                0 => break,
                b => text.push(b),
            }
        }
        let mut line = format!("[{}] row {row}: ", self.n).into_bytes();
        line.extend_from_slice(&text);
        line.push(b'\n');
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(file)
            .and_then(|mut fh| fh.write_all(&line))
            .map_err(|e| format!("skilltext: {}: {e}", file.display()))
    }

    /// `skillmap`: the class's action → skill-row table at `HERO+0x2002`
    /// (`<CLASS>::GetRealSkillIndex`, e.g. `WARRIOR` 0x145074). A belt slot
    /// holds the *action*, not the row, so `belt` wants the left number.
    fn skillmap(&mut self, vm: &mut Vm) {
        let Ok(hero) = Self::hero(vm, "skillmap") else {
            println!("  [{}] skillmap: not in gameplay", self.n);
            return;
        };
        let pairs: Vec<String> = (0..64u32)
            .map(|a| (a, vm.mem.read_u8(hero + 0x2002 + a) as i8))
            .filter(|&(_, r)| r >= 0)
            .map(|(a, r)| format!("{a}->{r}"))
            .collect();
        println!("  [{}] skillmap: {}", self.n, pairs.join(" "));
    }

    fn exp_buff(&mut self, vm: &mut Vm, ms: i64) -> Result<(), String> {
        const SET: &str = "_ZN4HERO14SetExpGoodTimeEx";
        const GV: &str = "gv";
        const GV_HERO: u32 = 0x1474;
        let f = vm.symbol(SET).ok_or_else(|| format!("no `{SET}` in this binary"))?;
        let gv = vm.symbol(GV).ok_or_else(|| format!("no `{GV}` in this binary"))?;
        let hero = vm.mem.read_u32(gv + GV_HERO);
        if hero == 0 {
            return Err("exp-buff: no HERO yet — the replay is not in gameplay".to_string());
        }
        let lo = (ms as u64 & 0xffff_ffff) as u32;
        let hi = ((ms as u64) >> 32) as u32;
        vm.call(f, &[hero, 0, lo, hi]).map(|_| ())
    }

    /// Bring the spirit 렌 out the way the story does, through the guest's own
    /// `EventProc::Event_PlayerAppearSpirit` (0x6DFB0): it writes the state
    /// byte at `HERO+0x1790` (0 none; 1 and 2 are not drawn; 3 is out). A new
    /// game has the spirit object (`Battle::LoadSpiritRes` runs on every
    /// battle) but state 0, and the story hands her over far past what a
    /// replay can reach — this is how the spirit avatar is seen offline.
    fn spirit(&mut self, vm: &mut Vm, state: i32) -> Result<(), String> {
        const APPEAR: &str = "_ZN9EventProc24Event_PlayerAppearSpiritEa";
        const GV_HERO: u32 = 0x1474;
        let f = vm.symbol(APPEAR).ok_or_else(|| format!("no `{APPEAR}` in this binary"))?;
        let gv = vm.symbol("gv").ok_or("no `gv` in this binary")?;
        if vm.mem.read_u32(gv + GV_HERO) == 0 {
            return Err("spirit: no HERO yet — the replay is not in gameplay".to_string());
        }
        // `this` is never read: the function reaches the hero through `gv`.
        vm.call(f, &[0, state as u32]).map(|_| ())
    }

    /// `npcbody <record>`: load `npc_g.dat` record N as the quest window's
    /// figure through the guest's own `QuestMgr::LoadNpcBody` — one of the
    /// four callers of `Map::NpcPathSet` (`hl5_core::npcbody`) — then print
    /// the sheets that call left in `_szSprFile[0..1]`. Record 0 is the
    /// player, record 1 is 렌; dressed, each shows its costume's sheet number
    /// instead of the equipment's / her own 042 and 025. No replay reaches a
    /// scene with her in it, so this is how her scene body is seen offline.
    fn npcbody(&mut self, vm: &mut Vm, record: i32) -> Result<(), String> {
        const LOAD: &str = "_ZN8QuestMgr11LoadNpcBodyEa";
        const SPR_FILE: &str = "_szSprFile";
        /// `gv + 0x1604` is the `QuestMgr*` (`StateInGameMenu::KeyQuestMenu+0x4a0`).
        const GV_QUEST_MGR: u32 = 0x1604;
        let f = vm.symbol(LOAD).ok_or_else(|| format!("no `{LOAD}` in this binary"))?;
        let gv = vm.symbol("gv").ok_or("no `gv` in this binary")?;
        let files = vm.symbol(SPR_FILE).ok_or("no `_szSprFile` in this binary")?;
        let qm = vm.mem.read_u32(gv + GV_QUEST_MGR);
        if qm == 0 {
            return Err("npcbody: no QuestMgr yet — the replay is not in gameplay".to_string());
        }
        vm.call(f, &[qm, record as u32])?;
        let path = |vm: &Vm, layer: u32| -> String {
            let p = files + layer * 0x100;
            let b: Vec<u8> = (0..0x40).map(|i| vm.mem.read_u8(p + i)).take_while(|c| *c != 0).collect();
            String::from_utf8_lossy(&b).into_owned()
        };
        println!("  npcbody {record}: layer 0 {} / layer 1 {}", path(vm, 0), path(vm, 1));
        Ok(())
    }

    /// Drop one table row at the hero's feet through the guest's own
    /// `MapItem::NewDropItem` — the same call every drop path ends in, so the
    /// `MakeSocket` net (`legendaries::fill_empty_options_on_drop`) sees the
    /// same return address it sees for a real drop. Then read the item back.
    ///
    /// Diagnostics: the alternative is to kill a monster whose ladder lands on
    /// the band in question, at ~0.25% per kill. `k2`/`v2` are the slot-2
    /// override a band-4/6 record carries (`lostdrops::slot2_override`);
    /// omitted means −1 = keep the row's byte, which for every 영웅 row is the
    /// reserved 0xFF.
    fn drop(&mut self, vm: &mut Vm, kind: i32, index: i32, k2: i32, v2: i32) -> Result<(), String> {
        const NEW_DROP: &str = "_ZN7MapItem11NewDropItemEiiaaaaaaaaa";
        const GET_X: &str = "_ZN6OBJECT4GetXEv";
        const GET_Y: &str = "_ZN6OBJECT4GetYEv";
        const GV: &str = "gv";
        const GV_MAP_ITEM: u32 = 0x1448;
        const GV_HERO: u32 = 0x1474;
        let f = vm.symbol(NEW_DROP).ok_or_else(|| format!("no `{NEW_DROP}` in this binary"))?;
        let gx = vm.symbol(GET_X).ok_or_else(|| format!("no `{GET_X}` in this binary"))?;
        let gy = vm.symbol(GET_Y).ok_or_else(|| format!("no `{GET_Y}` in this binary"))?;
        let gv = vm.symbol(GV).ok_or_else(|| format!("no `{GV}` in this binary"))?;
        let hero = vm.mem.read_u32(gv + GV_HERO);
        let map = vm.mem.read_u32(gv + GV_MAP_ITEM);
        if hero == 0 || map == 0 {
            return Err("drop: no HERO / MapItem yet — the replay is not in gameplay".to_string());
        }
        let x = vm.call(gx, &[hero])?;
        let y = vm.call(gy, &[hero])?;
        let sc = |v: i32| (v as i8 as u8) as u32;
        let ff = 0xffu32;
        let item = vm.call(
            f,
            // r0..r3 then eight stack bytes: index, grade, k0, v0, k1, v1, k2, v2.
            &[map, x, y, sc(kind), sc(index), ff, ff, ff, ff, ff, sc(k2), sc(v2)],
        )?;
        if item == 0 {
            println!("  [{}] drop kind={kind} index={index}: no item returned (picked up, or no free slot)", self.n);
            return Ok(());
        }
        let grade = vm.mem.read_u8(item + 0x15c) as i8;
        let kinds: Vec<i8> = (0..3).map(|s| vm.mem.read_u8(item + 0x15f + s) as i8).collect();
        let values: Vec<u8> = (0..3).map(|s| vm.mem.read_u8(item + 0x162 + s)).collect();
        let lines = match grade {
            g if g <= 0 => 0,
            1 => 1,
            2 => 2,
            _ => 3,
        };
        let empty: Vec<usize> = (0..lines).filter(|&s| kinds[s] <= 0).collect();
        println!(
            "  [{}] drop kind={kind} index={index}: grade={grade} option kinds={kinds:?} values={values:?}              lines={lines} empty={empty:?}{}",
            self.n,
            if empty.is_empty() { "" } else { "  <-- +랜덤옵션" }
        );
        Ok(())
    }

    /// `give <kind> <index> [n] [flag]`: make `n` items with the guest's own
    /// `ItemTable::NewItemEx` and put each in the bag with its own
    /// `BagItem::NewBagItem(item, flag)`, then save the bag the way the game
    /// does. `flag` is the guest's: 0 fills up to the bag's size, 1 lets the
    /// cash-shop path run into the overflow slots. Prints how many went in and
    /// the bag's count after — the way to fill a bag to a known size without
    /// playing for it.
    fn give(&mut self, vm: &mut Vm, kind: i32, index: i32, n: i32, flag: i32) -> Result<(), String> {
        const NEW_ITEM: &str = "_ZN9ItemTable9NewItemExEaa";
        const NEW_BAG: &str = "_ZN7BagItem10NewBagItemEP8ItemInfoh";
        const GET_BAG: &str = "_ZN4HERO13GetBagItemPtrEv";
        const SAVE_BAG: &str = "_ZN4HERO11SaveBagDataEa";
        const GV: &str = "gv";
        const GV_ITEM_TABLE: u32 = 0x1444;
        const GV_HERO: u32 = 0x1474;
        const GV_SAVE_SLOT: u32 = 0x1970;
        let sym = |s: &str| vm.symbol(s).ok_or_else(|| format!("no `{s}` in this binary"));
        let (new_item, new_bag, get_bag, save, gv) =
            (sym(NEW_ITEM)?, sym(NEW_BAG)?, sym(GET_BAG)?, sym(SAVE_BAG)?, sym(GV)?);
        let hero = vm.mem.read_u32(gv + GV_HERO);
        let table = vm.mem.read_u32(gv + GV_ITEM_TABLE);
        if hero == 0 || table == 0 {
            return Err("give: no HERO / ItemTable yet — the replay is not in gameplay".to_string());
        }
        let bag = vm.call(get_bag, &[hero])?;
        let sc = |v: i32| (v as i8 as u8) as u32;
        let mut put = 0;
        for _ in 0..n.max(1) {
            let item = vm.call(new_item, &[table, sc(kind), sc(index)])?;
            if item == 0 || vm.call(new_bag, &[bag, item, flag as u32 & 0xff])? == 0 {
                break;
            }
            put += 1;
        }
        let slot = vm.mem.read_u8(gv + GV_SAVE_SLOT) as i8 as u32;
        vm.call(save, &[hero, slot])?;
        let size = vm.mem.read_u16(bag + 4) as i16;
        let count = vm.mem.read_u16(bag + 6) as i16;
        println!(
            "  [{}] give kind={kind} index={index}: {put}/{} in, bag {count}/{size}",
            self.n,
            n.max(1)
        );
        Ok(())
    }

    /// `bagdump [file]`: the bag as the guest holds it — size, count, and per
    /// slot `kind:index x stack` — on stdout, or into `file` so two runs (save,
    /// then a fresh boot that loads it) can be compared byte for byte.
    fn bagdump(&mut self, vm: &mut Vm, file: Option<&str>) -> Result<(), String> {
        const GV_HERO: u32 = 0x1474;
        let gv = vm.symbol("gv").ok_or("no gv")?;
        let hero = vm.mem.read_u32(gv + GV_HERO);
        if hero == 0 {
            return Err("bagdump: no HERO yet".into());
        }
        let bag = hero + 0x17c0;
        let ten = vm.cfg.ten_pages;
        let (slots, stacks) = if ten { (172, 0x2b8) } else { (96, 0x188) };
        let size = vm.mem.read_u16(bag + 4) as i16;
        let count = vm.mem.read_u16(bag + 6) as i16;
        let mut out = format!("size {size} count {count}
");
        // HERO+0x1B2C: the eight equipment slots, each a bag index (i16).
        let equip: Vec<i16> = (0..8).map(|k| vm.mem.read_u16(hero + 0x1b2c + 2 * k) as i16).collect();
        out.push_str(&format!("equip {equip:?}
"));
        for i in 0..slots {
            let item = vm.mem.read_u32(bag + 8 + 4 * i);
            if item == 0 {
                continue;
            }
            let kind = vm.mem.read_u8(item + 0x14) as i8;
            let index = vm.mem.read_u8(item + 0x15) as i8;
            let n = vm.mem.read_u8(bag + stacks + i) as i8;
            // ItemInfo+0x165: the refine level, the one byte 정련 changes.
            let refine = vm.mem.read_u8(item + 0x165);
            // ItemInfo+0x168: socket count, +0x169..+0x16D the five slots.
            let sockets = if (0..=9).contains(&kind) {
                let b: Vec<String> = (0..6).map(|k| format!("{:02x}", vm.mem.read_u8(item + 0x168 + k))).collect();
                format!(" s{}", b.join(""))
            } else {
                String::new()
            };
            out.push_str(&format!("{i:3} {kind}:{index} x{n} r{refine}{sockets}
"));
        }
        // The filtered list the 정련/오브/장비 screens page through: each
        // entry's bag slot (`BagItemFilter+0x12`), and the chosen position
        // `gv+0x1B3E` (0xFF/−1 = none).
        let (filter, flen) = if ten { (bag + 0x860, 172) } else { (bag + 0x1e8, 96) };
        let fcount = vm.mem.read_u16(bag + 0x368) as i16;
        let slots_of: Vec<i16> = (0..(fcount.max(0) as u32).min(flen))
            .map(|k| {
                let e = vm.mem.read_u32(filter + 4 * k);
                if e == 0 { -1 } else { vm.mem.read_u16(e + 0x12) as i16 }
            })
            .collect();
        out.push_str(&format!(
            "filter {fcount} chosen {} {slots_of:?}
",
            vm.mem.read_u8(gv + 0x1b3e)
        ));
        match file {
            Some(f) => {
                std::fs::write(f, &out).map_err(|e| format!("{f}: {e}"))?;
                println!("  [{}] bagdump: size {size} count {count} -> {f}", self.n);
            }
            None => print!("{out}"),
        }
        Ok(())
    }

    fn npc_return(&mut self, vm: &mut Vm) -> Result<(), String> {
        const RET: &str = "_ZN9BaseState11ReturnStateEv";
        let f = vm.symbol(RET).ok_or_else(|| format!("no `{RET}` in this binary"))?;
        vm.call(f, &[0]).map(|_| ())
    }

    fn run_script(&mut self, vm: &mut Vm, text: &str) -> Result<(), String> {
        for (ln, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut words = line.split_whitespace();
            let cmd = words.next().unwrap();
            let rest: Vec<&str> = words.collect();
            let at = |i: usize| -> Result<i32, String> {
                rest.get(i)
                    .ok_or_else(|| format!("line {}: `{cmd}` wants {} numbers", ln + 1, i + 1))?
                    .parse()
                    .map_err(|_| format!("line {}: `{}` is not a number", ln + 1, rest[i]))
            };
            // The pointer id of a `press` / `release` / `move`, which the
            // guest uses to decide which finger owns the touch joystick
            // (ANALYSIS.md §37.5). Optional and zero by default, so every
            // script written before it existed replays a single finger and
            // sends exactly the bytes it always did.
            let pointer = |i: usize| -> Result<i32, String> {
                match rest.get(i) {
                    None => Ok(0),
                    Some(s) => s
                        .parse()
                        .map_err(|_| format!("line {}: pointer id `{s}` is not a number", ln + 1)),
                }
            };
            // A key argument is either a WIPI name or the raw code, so a code
            // this table does not name is still reachable.
            let keycode = || -> Result<i32, String> {
                let k = rest
                    .first()
                    .ok_or_else(|| format!("line {}: `{cmd}` wants a key", ln + 1))?;
                hl5_core::wipi_key(k)
                    .or_else(|| k.parse().ok())
                    .ok_or_else(|| format!("line {}: unknown key `{k}`", ln + 1))
            };
            match cmd {
                "frames" => self.frames(vm, at(0)?.max(0) as u32)?,
                "tap" => {
                    let (x, y) = (at(0)?, at(1)?);
                    let p = pointer(2)?;
                    vm.touch(Touch::Pressed, x, y, p)?;
                    self.frames(vm, 1)?;
                    vm.touch(Touch::Released, x, y, p)?;
                    self.frames(vm, 1)?;
                }
                "press" => vm.touch(Touch::Pressed, at(0)?, at(1)?, pointer(2)?)?,
                "release" => vm.touch(Touch::Released, at(0)?, at(1)?, pointer(2)?)?,
                "move" => vm.touch(Touch::Moved, at(0)?, at(1)?, pointer(2)?)?,
                "key" => {
                    let k = keycode()?;
                    vm.key(k, true)?;
                    self.frames(vm, 1)?;
                    vm.key(k, false)?;
                    self.frames(vm, 1)?;
                }
                "keydown" => vm.key(keycode()?, true)?,
                "keyup" => vm.key(keycode()?, false)?,
                "dump" => {
                    let path = PathBuf::from(rest.first().copied().unwrap_or("out/frame.png"));
                    if let Some(dir) = path.parent() {
                        if !dir.as_os_str().is_empty() {
                            std::fs::create_dir_all(dir).ok();
                        }
                    }
                    dump_png(vm, &path)?;
                    println!("  [{}] wrote {}", self.n, path.display());
                    // Also into the log, so a `--watch` line can be placed
                    // between two dumps.
                    vm.log.push(format!("[{}] wrote {}", self.n, path.display()));
                }
                "echo" => {
                    println!("  [{}] {}", self.n, rest.join(" "));
                    // And into the log, so a `--watch` line can be placed
                    // relative to a script line.
                    vm.log.push(format!("[{}] {}", self.n, rest.join(" ")));
                }
                // ---- gamepad -------------------------------------------
                //
                // `pad <keycode|name> [down|up]` with no third word is a full
                // press: down, a frame, up, a frame — the same shape as `key`.
                // `pad-stick <x> <y>` takes -1.0..1.0 per axis, and
                // `pad-connect` / `pad-hud` drive what the badges do.
                "pad" => {
                    let code = pad_code(rest.first().copied().unwrap_or(""))
                        .ok_or_else(|| format!("line {}: unknown pad button `{}`", ln + 1, rest.first().copied().unwrap_or("")))?;
                    match rest.get(1).copied() {
                        Some("down") => {
                            vm.pad_button(code, true, false);
                            self.frames(vm, 1)?;
                        }
                        Some("up") => {
                            vm.pad_button(code, false, false);
                            self.frames(vm, 1)?;
                        }
                        None => {
                            vm.pad_button(code, true, false);
                            self.frames(vm, 2)?;
                            vm.pad_button(code, false, false);
                            self.frames(vm, 2)?;
                        }
                        Some(other) => {
                            return Err(format!("line {}: `pad` wants down/up, not `{other}`", ln + 1))
                        }
                    }
                }
                "pad-stick" => {
                    let f = |i: usize| -> Result<f32, String> {
                        rest.get(i)
                            .ok_or_else(|| format!("line {}: `pad-stick` wants x and y", ln + 1))?
                            .parse()
                            .map_err(|_| format!("line {}: `{}` is not a number", ln + 1, rest[i]))
                    };
                    vm.pad_axes(f(0)?, f(1)?, 0.0, 0.0, 0.0, 0.0);
                }
                "pad-connect" => vm.pad_present(rest.first().copied() != Some("off")),
                "pad-hud" => {
                    vm.pad.hud = hl5_core::padhud::PadHud::parse(rest.first().copied().unwrap_or("auto"));
                    // The badges are part of the picture, so their appearing
                    // has to invalidate the host's same-picture shortcut.
                    vm.gl.forget_last_draw();
                }
                "pad-release" => vm.pad_release(),
                "npc-menu" => self.npc_menu(vm, at(0)?)?,
                "exp-buff" => self.exp_buff(vm, i64::from(at(0)?))?,
                "learn" => self.learn(vm, at(0)?, at(1)?)?,
                "belt" => self.belt(vm, at(0)?, at(1)?, at(2)?)?,
                "buffs" => self.buffs(vm, rest.first().and_then(|r| r.parse().ok())),
                "skillmap" => self.skillmap(vm),
                "sp" => self.sp(vm, at(0)?)?,
                "skilltext" => {
                    let path = rest.get(1).ok_or_else(|| format!("line {}: `skilltext` wants <row> <file>", ln + 1))?;
                    self.skilltext(vm, at(0)?, Path::new(path))?
                }
                "spirit" => self.spirit(vm, at(0)?)?,
                "npcbody" => self.npcbody(vm, at(0)?)?,
                "npc-return" => self.npc_return(vm)?,
                // `hudaudit`: what each 배치변경 control draws, and anything
                // the regions miss (see `hudlayout::audit`).
                "hudaudit" => {
                    for line in hl5_core::hudlayout::audit(vm)? {
                        println!("  [{}] {line}", self.n);
                    }
                }
                "bagdump" => self.bagdump(vm, rest.first().copied())?,
                // `class <0..3>`: the guest's own `StateInGameMenu::ChangeHeroClass`,
                // which the class-change books call from the bag in town. It
                // overwrites `r0` in its third instruction, so `this` is never
                // read and the replay can reach it from a dungeon save.
                "class" => {
                    const F: &str = "_ZN15StateInGameMenu15ChangeHeroClassEa";
                    let f = vm.symbol(F).ok_or_else(|| format!("no `{F}` in this binary"))?;
                    vm.call(f, &[0, at(0)? as u32])?;
                    println!("  [{}] class -> {}", self.n, at(0)?);
                }
                // `menu-substate <n>`: the in-game menu's own
                // `StateInGameMenu::SetSubState` on the state on screen — to
                // reach a page's belt-registration substate (스킬 6/7) without
                // walking the menus that lead there.
                "menu-substate" => {
                    const F: &str = "_ZN15StateInGameMenu11SetSubStateEa";
                    let f = vm.symbol(F).ok_or_else(|| format!("no `{F}` in this binary"))?;
                    let gv = vm.symbol("gv").ok_or("no gv")?;
                    let st = vm.mem.read_u32(gv + 0x27c);
                    vm.call(f, &[st, at(0)? as u32])?;
                    println!("  [{}] menu substate -> {}", self.n, at(0)?);
                }
                // `money <n>`: the guest's own `BagItem::IncreaseMoney` on the
                // hero's bag — for scenarios that pay (정련).
                "money" => {
                    const F: &str = "_ZN7BagItem13IncreaseMoneyEi";
                    let f = vm.symbol(F).ok_or_else(|| format!("no `{F}` in this binary"))?;
                    let gv = vm.symbol("gv").ok_or("no gv")?;
                    let hero = vm.mem.read_u32(gv + 0x1474);
                    if hero == 0 {
                        return Err("money: no HERO yet".into());
                    }
                    vm.call(f, &[hero + 0x17c0, at(0)? as u32])?;
                    println!("  [{}] money +{}", self.n, at(0)?);
                }
                "give" => {
                    let n = if rest.len() > 2 { at(2)? } else { 1 };
                    let flag = if rest.len() > 3 { at(3)? } else { 0 };
                    self.give(vm, at(0)?, at(1)?, n, flag)?
                }
                "drop" => {
                    let (k2, v2) = match (rest.get(2), rest.get(3)) {
                        (Some(_), Some(_)) => (at(2)?, at(3)?),
                        _ => (-1, -1),
                    };
                    self.drop(vm, at(0)?, at(1)?, k2, v2)?
                }
                other => return Err(format!("line {}: unknown command `{other}`", ln + 1)),
            }
        }
        Ok(())
    }
}

/// A `pad` script argument: an Android keycode, or the POSITION name the
/// badges use. Names and not letters for the reason `padhud` gives — keycode
/// 96 is printed `A` on an Xbox pad and `B` on a Switch pad — so a replay
/// reads as "press the bottom face button".
fn pad_code(name: &str) -> Option<i32> {
    use hl5_core::gamepad::*;
    Some(match name.to_ascii_lowercase().as_str() {
        "a" | "face-down" | "down-face" => KEYCODE_BUTTON_A,
        "b" | "face-right" => KEYCODE_BUTTON_B,
        "x" | "face-left" => KEYCODE_BUTTON_X,
        "y" | "face-up" => KEYCODE_BUTTON_Y,
        "l1" | "lb" => KEYCODE_BUTTON_L1,
        "r1" | "rb" => KEYCODE_BUTTON_R1,
        "l2" | "lt" => KEYCODE_BUTTON_L2,
        "r2" | "rt" => KEYCODE_BUTTON_R2,
        "l3" | "thumbl" => KEYCODE_BUTTON_THUMBL,
        "r3" | "thumbr" => KEYCODE_BUTTON_THUMBR,
        "select" => KEYCODE_BUTTON_SELECT,
        "start" => KEYCODE_BUTTON_START,
        "up" => KEYCODE_DPAD_UP,
        "down" => KEYCODE_DPAD_DOWN,
        "left" => KEYCODE_DPAD_LEFT,
        "right" => KEYCODE_DPAD_RIGHT,
        // The 세로 keypad, by the label on the key: `pad key:5`, `pad key:대시`.
        other => {
            if let Some(label) = other.strip_prefix("key:") {
                let i = hl5_core::portrait::KEYPAD.iter().position(|(k, _)| *k == label)?;
                return Some(hl5_core::portrait::KEYPAD_CODE_BASE + i as i32);
            }
            return other.parse().ok();
        }
    })
}

// ---- audio clip dump --------------------------------------------------------
//
// The host never decodes a clip — the platform does, exactly as in the
// original — so the only thing worth checking headlessly is whether the byte
// range the seam recorded is the right one. Writing each clip out answers
// that in the strongest available way: a file that starts with `OggS` and
// plays in any decoder means `putSoundClip`'s offset and length were read
// correctly out of the guest's `long long` arguments.

fn dump_audio(vm: &Vm, dir: &std::path::Path) -> Result<(), String> {
    if vm.audio.is_empty() {
        println!("\n== audio ==\n  no clips registered");
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    println!("\n== audio ==");
    let mut ogg = 0usize;
    for (key, clip) in vm.audio.iter() {
        let (tag, bytes): (&str, Vec<u8>) = match &clip.pcm {
            Some(p) => ("pcm", p.data.clone()),
            None => {
                let start = clip.offset as usize;
                let end = start.saturating_add(clip.length as usize).min(vm.cfg.vfs.len());
                let slice = vm.cfg.vfs.get(start..end).unwrap_or(&[]).to_vec();
                (if clip.kind == hl5_core::audio::ClipKind::Music { "music" } else { "sound" },
                 slice)
            }
        };
        // `OggS` is the Ogg page capture pattern; the archive's clips are Ogg
        // Vorbis, so anything else here is a wrong offset, not a new format.
        let looks_ogg = bytes.starts_with(b"OggS");
        if looks_ogg {
            ogg += 1;
        }
        let ext = if looks_ogg { "ogg" } else { "bin" };
        let path = dir.join(format!("{tag}_{key:016x}.{ext}"));
        std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        println!(
            "  {tag:<5} key {key:#018x}  vfs[{}..{}]  {} bytes  plays {}  {}",
            clip.offset,
            clip.offset + clip.length,
            bytes.len(),
            clip.plays,
            if looks_ogg { "OggS" } else { "?" }
        );
    }
    println!("  {ogg}/{} start with the Ogg capture pattern", vm.audio.len());
    Ok(())
}

// ---- framebuffer PNG dump --------------------------------------------------
//
// A dependency-free PNG writer: one IDAT with a stored (uncompressed) zlib
// stream. The framebuffer is small (400x240), so no real compression is
// needed to get a file the S9 reference captures can be diffed against.

fn dump_png(vm: &Vm, path: &std::path::Path) -> Result<(), String> {
    let (w, h) = (vm.gl.fb_w as usize, vm.gl.fb_h as usize);
    // Raw image: filter byte 0 per row, then RGB triples.
    let mut raw = Vec::with_capacity(h * (1 + w * 3));
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            let px = vm.gl.fb[y * w + x];
            raw.push((px >> 16) as u8);
            raw.push((px >> 8) as u8);
            raw.push(px as u8);
        }
    }

    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, colour type 2 (RGB)
    write_chunk(&mut png, b"IHDR", &ihdr);
    write_chunk(&mut png, b"IDAT", &zlib_stored(&raw));
    write_chunk(&mut png, b"IEND", &[]);

    std::fs::write(path, png).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc = crc32(tag);
    crc = crc32_update(crc, data);
    out.extend_from_slice(&(crc ^ 0xffff_ffff).to_be_bytes());
}

/// zlib stream wrapping `data` in uncompressed DEFLATE blocks + Adler-32.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // zlib header, no dict
    for (i, chunk) in data.chunks(0xffff).enumerate().collect::<Vec<_>>().iter() {
        let last = (i + 1) * 0xffff >= data.len();
        out.push(if last { 1 } else { 0 });
        let n = chunk.len() as u16;
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&(!n).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn crc32(seed: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, seed)
}

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    crc
}

// ---- HLE self-test ---------------------------------------------------------
//
// Drives the HLE layer through the same door the guest uses: `Vm::call` on a
// trampoline address, so argument decoding, the dispatch match and the return
// path are all exercised — not the Rust functions in isolation. Anything the
// guest could observe differently, this can catch.

fn selftest(vm: &mut Vm) -> Result<(), String> {
    println!("\n== HLE self-test (through trampolines) ==");
    let mut passed = 0u32;

    // helpers -----------------------------------------------------------
    let tramp = |vm: &Vm, name: &str| -> Result<u32, String> {
        vm.imports
            .iter()
            .position(|n| n == name)
            .map(hl5_core::hle::tramp_addr)
            .ok_or_else(|| format!("selftest: guest does not import `{name}`"))
    };
    fn put(vm: &mut Vm, bytes: &[u8]) -> u32 {
        let p = vm.heap.alloc(bytes.len().max(1) as u32).expect("selftest heap");
        vm.mem.write_bytes(p, bytes);
        p
    }
    fn check(passed: &mut u32, what: &str, ok: bool, detail: String) -> Result<(), String> {
        if ok {
            println!("  ok   {what}  ({detail})");
            *passed += 1;
            Ok(())
        } else {
            Err(format!("selftest FAILED: {what} — {detail}"))
        }
    }

    // vsnprintf: flags, width, precision, %s passthrough, %f from a va_list
    {
        let fmt = put(vm, b"x=%d y=%08x s=%s f=%.2f\0");
        let s = put(vm, b"hi\0");
        // va_list area: 3 words, then the double 8-aligned.
        let ap = vm.heap.alloc(32).unwrap();
        let ap = (ap + 7) & !7;
        vm.mem.write_u32(ap, 42);
        vm.mem.write_u32(ap + 4, 0xdead);
        vm.mem.write_u32(ap + 8, s);
        let bits = 3.5f64.to_bits();
        vm.mem.write_u32(ap + 16, bits as u32);
        vm.mem.write_u32(ap + 20, (bits >> 32) as u32);
        let buf = vm.heap.alloc(64).unwrap();
        let t = tramp(vm, "vsnprintf")?;
        let n = vm.call(t, &[buf, 64, fmt, ap])?;
        let out = vm.mem.cstr(buf);
        check(
            &mut passed,
            "vsnprintf",
            out == "x=42 y=0000dead s=hi f=3.50" && n == out.len() as u32,
            format!("{out:?}, ret {n}"),
        )?;
    }

    // qsort: guest-callable comparator (the strcmp trampoline) over 8-byte keys
    {
        let base = put(vm, b"pear\0\0\0\0apple\0\0\0mango\0\0\0");
        let cmp = tramp(vm, "strcmp")?;
        let t = tramp(vm, "qsort")?;
        vm.call(t, &[base, 3, 8, cmp])?;
        let got = vm.mem.slice(base, 24).unwrap().to_vec();
        check(
            &mut passed,
            "qsort (re-entrant comparator)",
            &got == b"apple\0\0\0mango\0\0\0pear\0\0\0\0",
            format!("{:?}", String::from_utf8_lossy(&got)),
        )?;
    }

    // libm: soft-float doubles across register pairs, both directions
    {
        let f = |v: f64| (v.to_bits() as u32, (v.to_bits() >> 32) as u32);
        let (plo, phi) = f(2.0);
        let (qlo, qhi) = f(10.0);
        let t = tramp(vm, "pow")?;
        let (r0, r1) = vm.call2(t, &[plo, phi, qlo, qhi])?;
        let got = f64::from_bits(r0 as u64 | (r1 as u64) << 32);
        check(&mut passed, "pow(2,10)", got == 1024.0, format!("{got}"))?;

        let (clo, chi) = f(2.3);
        let t = tramp(vm, "ceil")?;
        let (r0, r1) = vm.call2(t, &[clo, chi])?;
        let got = f64::from_bits(r0 as u64 | (r1 as u64) << 32);
        check(&mut passed, "ceil(2.3)", got == 3.0, format!("{got}"))?;
    }

    // files: create → write → reopen → read → stat → unlink, all guest-side
    {
        let path = put(vm, b"/data/data/co.kr.eamobile.HeroesLore5/files/selftest.sav\0");
        let (o_wr_creat_trunc, o_rd) = (0o1 | 0o100 | 0o1000, 0);
        let topen = tramp(vm, "open")?;
        let fd = vm.call(topen, &[path, o_wr_creat_trunc, 0o644])?;
        check(&mut passed, "open(O_CREAT)", fd >= 3, format!("fd {fd}"))?;
        let data = put(vm, b"SAVEDATA");
        let n = vm.call(tramp(vm, "write")?, &[fd, data, 8])?;
        check(&mut passed, "write", n == 8, format!("wrote {n}"))?;
        vm.call(tramp(vm, "close")?, &[fd])?;

        let fd = vm.call(topen, &[path, o_rd, 0])?;
        let back = vm.heap.alloc(16).unwrap();
        let n = vm.call(tramp(vm, "read")?, &[fd, back, 16])?;
        let got = vm.mem.slice(back, 8).unwrap().to_vec();
        check(&mut passed, "read back", n == 8 && got == b"SAVEDATA", format!("{n} bytes"))?;
        vm.call(tramp(vm, "close")?, &[fd])?;

        let st = vm.heap.alloc(104).unwrap();
        let r = vm.call(tramp(vm, "stat")?, &[path, st])?;
        let size = vm.mem.read_u32(st + 48);
        check(&mut passed, "stat st_size", r == 0 && size == 8, format!("size {size}"))?;

        let r = vm.call(tramp(vm, "unlink")?, &[path])?;
        let r2 = vm.call(tramp(vm, "access")?, &[path, 0])?;
        let errno = vm.mem.read_u32(hl5_core::hle::ERRNO_ADDR);
        check(
            &mut passed,
            "unlink + access → ENOENT",
            r == 0 && r2 == u32::MAX && errno == 2,
            format!("access {r2:#x}, errno {errno}"),
        )?;
    }

    // VFS by name: the archive must come back byte-identical from memory
    if !vm.cfg.vfs.is_empty() {
        let path = put(vm, b"/data/data/co.kr.eamobile.HeroesLore5/files/data.vfs.mp3\0");
        let fd = vm.call(tramp(vm, "open")?, &[path, 0, 0])?;
        let buf = vm.heap.alloc(16).unwrap();
        let n = vm.call(tramp(vm, "read")?, &[fd, buf, 16])?;
        let got = vm.mem.slice(buf, 16).unwrap().to_vec();
        let want = vm.cfg.vfs[..16].to_vec();
        let end = vm.call(tramp(vm, "lseek")?, &[fd, 0, 2])?;
        vm.call(tramp(vm, "close")?, &[fd])?;
        check(
            &mut passed,
            "VFS served from memory",
            n == 16 && got == want && end == vm.cfg.vfs.len() as u32,
            format!("first 16 B match, lseek(END) = {end}"),
        )?;
    }

    // sockets: the servers-are-gone world
    {
        let fd = vm.call(tramp(vm, "socket")?, &[2, 1, 0])?;
        let r = vm.call(tramp(vm, "connect")?, &[fd, 0, 0])?;
        let errno = vm.mem.read_u32(hl5_core::hle::ERRNO_ADDR);
        check(
            &mut passed,
            "connect fails (dead server)",
            r == u32::MAX && errno == 111,
            format!("errno {errno}"),
        )?;
        let before = vm.now_ms();
        let tv = vm.heap.alloc(8).unwrap();
        vm.mem.write_u32(tv, 2); // 2 s
        vm.mem.write_u32(tv + 4, 0);
        let r = vm.call(tramp(vm, "select")?, &[0, 0, 0, 0, tv])?;
        let waited = vm.now_ms() - before;
        check(
            &mut passed,
            "select times out on the virtual clock",
            r == 0 && waited == 2000,
            format!("advanced {waited} ms"),
        )?;
        let ip = put(vm, b"127.0.0.1\0");
        let a = vm.call(tramp(vm, "inet_addr")?, &[ip])?;
        check(&mut passed, "inet_addr", a == 0x0100_007f, format!("{a:#010x}"))?;
    }

    // strdup / strerror / __errno / ftime
    {
        let s = put(vm, b"midas\0");
        let d = vm.call(tramp(vm, "strdup")?, &[s])?;
        let got = vm.mem.cstr(d);
        check(&mut passed, "strdup", d != s && got == "midas", format!("{got:?}"))?;

        let p = vm.call(tramp(vm, "strerror")?, &[2])?;
        let got = vm.mem.cstr(p);
        check(&mut passed, "strerror(ENOENT)", got.contains("No such"), got)?;

        let e = vm.call(tramp(vm, "__errno")?, &[])?;
        check(
            &mut passed,
            "__errno",
            e == hl5_core::hle::ERRNO_ADDR,
            format!("{e:#010x}"),
        )?;

        let tb = vm.heap.alloc(12).unwrap();
        vm.call(tramp(vm, "ftime")?, &[tb])?;
        let sec = vm.mem.read_u32(tb);
        let want = (vm.now_ms() / 1000) as u32;
        check(&mut passed, "ftime", sec == want, format!("sec {sec}"))?;
    }

    // GLES capture: names, upload, and a blit into the host framebuffer
    {
        let names = vm.heap.alloc(8).unwrap();
        vm.call(tramp(vm, "glGenTextures")?, &[2, names])?;
        let (n1, n2) = (vm.mem.read_u32(names), vm.mem.read_u32(names + 4));
        check(&mut passed, "glGenTextures", n1 != 0 && n2 != 0 && n1 != n2, format!("{n1}, {n2}"))?;

        vm.call(tramp(vm, "glBindTexture")?, &[0x0DE1, n1])?;
        // 2x2 RGB565, all pixels pure red (0xF800)
        let px = put(vm, &[0x00, 0xF8, 0x00, 0xF8, 0x00, 0xF8, 0x00, 0xF8]);
        vm.call(
            tramp(vm, "glTexImage2D")?,
            &[0x0DE1, 0, 0x1907, 2, 2, 0, 0x1907, 0x8363, px],
        )?;
        check(
            &mut passed,
            "glTexImage2D stored",
            vm.gl.textures.get(&n1).is_some_and(|t| t.pixels.len() == 8),
            "2x2 RGB565".into(),
        )?;

        // A full-screen quad through ortho(0..400, 240..0), viewport = fb
        vm.call(tramp(vm, "glMatrixMode")?, &[0x1701])?;
        vm.call(tramp(vm, "glLoadIdentity")?, &[])?;
        let f = |v: f32| v.to_bits();
        vm.call(
            tramp(vm, "glOrthof")?,
            &[f(0.0), f(400.0), f(240.0), f(0.0), f(-1.0), f(1.0)],
        )?;
        let (fbw, fbh) = (vm.gl.fb_w as u32, vm.gl.fb_h as u32);
        vm.call(tramp(vm, "glViewport")?, &[0, 0, fbw, fbh])?;
        vm.call(tramp(vm, "glEnableClientState")?, &[0x8074])?;
        vm.call(tramp(vm, "glEnableClientState")?, &[0x8078])?;
        // triangle-strip quad, GL_FLOAT xy
        let verts: Vec<u8> = [0.0f32, 0.0, 400.0, 0.0, 0.0, 240.0, 400.0, 240.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let tcs: Vec<u8> = [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let vp = put(vm, &verts);
        let tp = put(vm, &tcs);
        vm.call(tramp(vm, "glVertexPointer")?, &[2, 0x1406, 0, vp])?;
        vm.call(tramp(vm, "glTexCoordPointer")?, &[2, 0x1406, 0, tp])?;
        vm.call(tramp(vm, "glDrawArrays")?, &[5, 0, 4])?;
        let mid = vm.gl.fb[(vm.gl.fb_h / 2 * vm.gl.fb_w + vm.gl.fb_w / 2) as usize];
        check(
            &mut passed,
            "glDrawArrays blits the texture",
            mid == 0x00ff_0000,
            format!("centre pixel {mid:#08x}"),
        )?;
    }

    // audio clip table: the handle is the guest's own `long`, the returned id
    // is SoundPool's (0 means the load failed), and status/play answer the way
    // `MidasSoundManager` + `WIPIConstant.getWIPIMediaStatus` do.
    {
        use hl5_core::audio::{Clip, ClipKind, ClipState};
        let clip = |offset: u64, length: u64| Clip {
            kind: ClipKind::Sound,
            state: ClipState::Stopped,
            looping: false,
            offset,
            length,
            hint: 0,
            pcm: None,
            plays: 0,
        };
        let key = 0x1234_5678_9abc_def0u64;
        let id = vm.audio.put(key, clip(64, 128));
        check(&mut passed, "put*Clip returns a non-zero SoundPool id", id != 0,
              format!("id {id}"))?;
        check(&mut passed, "an unloaded handle has no status",
              vm.audio.status(key ^ 1) == u32::MAX, "status -1".into())?;
        check(&mut passed, "playing an unloaded handle fails",
              vm.audio.play(key ^ 1, false) == u32::MAX, "play -1".into())?;

        vm.audio.play(key, true);
        let c = vm.audio.get(key).expect("just registered");
        check(&mut passed, "clipPlay marks the clip playing and looping",
              c.state == ClipState::Playing && c.looping && c.plays == 1,
              format!("{:?} plays {}", c.state, c.plays))?;
        check(&mut passed, "getClipStatus reports 2 while playing",
              vm.audio.status(key) == 2, "status 2".into())?;

        vm.audio.pause(key);
        check(&mut passed, "clipPause only pauses a playing clip",
              vm.audio.get(key).unwrap().state == ClipState::Paused, "paused".into())?;
        vm.audio.resume(key);
        check(&mut passed, "clipResume returns it to playing",
              vm.audio.get(key).unwrap().state == ClipState::Playing, "playing".into())?;
        vm.audio.free(key);
        check(&mut passed, "clipFree drops the handle",
              vm.audio.get(key).is_none() && vm.audio.status(key) == u32::MAX,
              "gone".into())?;

        // SoundPool.load fails on an empty range, and saying so lets the
        // guest's own retry-and-log path run instead of silently doing nothing.
        check(&mut passed, "an empty byte range refuses to load",
              vm.audio.put(1, clip(0, 0)) == 0, "id 0".into())?;
    }

    println!("\nOK — {passed} checks passed, {} HLE calls served", vm.hle_calls);
    Ok(())
}
