# Build the heroesLore5 64-bit APK.
#
#   .\build_android.ps1                  debug APK
#   .\build_android.ps1 -Install         ... and push it to the attached device
#   .\build_android.ps1 -Release         signed release (see the keystore notes
#                                        in android_app/app/build.gradle.kts)
#   .\build_android.ps1 -NewKeystore     create a release keystore, print how to
#                                        wire it up, and stop
#
# What it does, in order: build the dlltool shim this host needs, cross-compile
# hl5_android to aarch64, refresh the bundled payload in assets/, then gradle.

param(
    [switch]$Release,
    [switch]$Install,
    [switch]$NewKeystore,
    [switch]$SkipRust,
    # adb serial. Required when more than one device is attached, which is the
    # normal state on a bench with a phone and a tablet plugged in.
    [string]$Device,
    # Follow the app's logcat after installing.
    [switch]$Logcat,
    # Release-optimised Rust inside a debug-signed APK. `-Release` produces an
    # UNSIGNED apk unless a keystore is configured, and a plain debug build
    # compiles the interpreter without LTO — on a phone that is roughly half
    # speed, so it is not a number worth quoting. This is the combination to
    # measure with and to hand to a tester.
    [switch]$Fast,
    # Bake a default private-server address into the build
    # (`host:port`). The app prefers `<files>/hl5_server.txt` when it is
    # there, so this is the fallback a tester's build carries. A release
    # APK is not debuggable, which means adb cannot write that file --
    # for those, this switch is the only way to point the app somewhere.
    [string]$Server,
    # Boot policy baked into the build: `original` (the guest's own boot,
    # Connecting and all -- what the reference A/B and the replay scripts
    # assume) or `skip-network` (jump past StateMainMenu states 0/1; the
    # default when this switch is absent, because "install it, open it, be
    # at the main screen" is the user requirement). The app prefers
    # `<files>/hl5_boot.txt` when it is there.
    [string]$Boot,
    # Guest clock rate baked into the build (`0.1`..`8.0`; unset means 1.0,
    # the game as written). The app prefers `<files>/hl5_speed.txt`, but a
    # release APK is not debuggable so adb cannot write that file -- this
    # switch is the only way to move the speed lever on the build a tester
    # measures with. §33.0's warning applies: do not test above 1.0 until
    # `repaints/s` shows the headroom.
    [string]$Speed,
    # Presenter baked into the build: `egl` (GPU, the default) or `soft`
    # (the CPU softbuffer path). The app prefers `<files>/hl5_present.txt`;
    # on a release APK this switch is the only way to force the old path,
    # e.g. on a device whose EGL misbehaves without failing outright.
    [string]$Present,
    # Which cores the guest thread may run on: `prime` (the default -- the
    # fastest cluster the device exposes) or `off` (leave the placement to
    # the scheduler, which on the bench S25 never used the prime cores at
    # all). The app prefers `<files>/hl5_affinity.txt`.
    [string]$Affinity,
    # Poll period in milliseconds (1..50; unset means 12). This is the grid
    # the guest is offered a `nativeLoop` on, and therefore the ceiling on
    # its tick rate. The app prefers `<files>/hl5_poll.txt`.
    [string]$Poll,
    # Whether releasing a key that is not a direction stops clearing the
    # hero's movement flags: `on` (the default) or `off` (the game exactly as
    # shipped, which ends the walk on any key release -- see
    # `hl5_core::input`). The app prefers `<files>/hl5_keep_walking.txt`.
    # This is the only lever on the fix, and a release APK is not debuggable,
    # so this switch is the way back to the original behaviour if the hook
    # misbehaves on a device it has never run on.
    [string]$KeepWalking,

    # Whether each on-screen field control says which pad button fires it:
    # `auto` (the default -- only while a pad is attached), `on`, `off`. The
    # app prefers `<files>/hl5_pad_hud.txt`, which a release APK cannot be
    # given, so this switch is how a signed build ships a different default.
    # It does not turn gamepad *input* off: that is always on, and what a
    # player remaps is `<files>/hl5_gamepad.txt`.
    [string]$PadHud,

    # Probability tables: `on` (the default) or `off` (the rates exactly as
    # shipped). On means the host lays down rewritten copies of
    # `probability.dat` (orb-combine success) and `enemy_<N>.dat` (the 전설 drop
    # band) for the guest to read -- see `hl5_core::rates`. The app prefers
    # `<files>/hl5_rates.txt`, which a release APK cannot be given, so this
    # switch is the way back.
    #
    # Turning it off does not undo it: the override files stay where they were
    # written and `LoadRes` keeps preferring them. Deleting them is what
    # restores the shipped rates.
    [string]$Rates,

    # -DumpSave on: read the player's save at boot and print it to logcat,
    # base64 encoded, so a location-dependent bug can be replayed offline.
    # See `hl5_core::savedump`.
    #
    # This exists because a release APK is not `debuggable`: `run-as` is
    # refused, so there is no other way to get the save off the device that
    # is actually running. It is off unless asked for, it only reads, and the
    # bytes are the player's -- do not ship a build with it on.
    #
    # Pull the save back with:
    #   adb -s <serial> logcat -d -s hl5 > save.log
    #   python tools/savedump_decode.py save.log out/storage
    [string]$DumpSave,

    # -CashItems off: leave out the cash-shop items the 2011 game never
    # shipped (슈퍼 성장의 서 and whatever follows it). See
    # `hl5_core::cashitems`. Default on; this is the way back on a release
    # APK, which cannot read <files>/hl5_cash_items.txt.
    [string]$CashItems,

    # -SkillTime off: arm buff durations and buff cooldowns in the guest's own
    # ticks again, so a "34초" buff ends after 14.2 s at 24 Hz (the shipped
    # game's arithmetic), and drop the x2 buff rebalance with it. See
    # `hl5_core::skilltime`. Default on; this is the
    # way back on a release APK, which cannot read <files>/hl5_skill_time.txt.
    [string]$SkillTime,

    # -TickCap <Hz> or -TickCap off: hold every device to the same served tick
    # rate. The hero moves a fixed number of pixels PER TICK and is never
    # time-scaled, so ticks/second IS movement speed -- and uncapped that means
    # a fast phone plays a faster game (S25 27.2 vs a throttled S9 12.3).
    # Default 24: above the 22.2 the guest's own timer asks for, below the
    # 28.57 the code can reach, so there is headroom for thermal droop.
    [string]$TickCap,

    # -Flavor vanilla: build the content line without the (진) weapons.
    # Normally left alone -- android_app/flavor.properties carries the value
    # for the branch you are on (empty on main, `vanilla` on vanilla), and
    # this switch only overrides it for a one-off build. It decides BOTH what
    # the binary does (HL5_FLAVOR -> Config::vanilla) and what the dist file
    # is called, so the two can never disagree. See BRANCHES.md.
    [string]$Flavor
)

# NB: deliberately NOT 'Stop'. cargo, gradle and adb all write progress to
# stderr, and with 'Stop' PowerShell turns the first such line into a
# terminating NativeCommandError — the build dies on "Building arm64-v8a". Every
# native call below is checked with $LASTEXITCODE instead, which is the thing
# that actually reports failure.
$ErrorActionPreference = 'Continue'
$root = $PSScriptRoot

# --- content line (BRANCHES.md) ---------------------------------------------
# -Flavor wins; otherwise the checkout's own flavor.properties decides. The
# value is baked into the binary AND put in the dist filename, from this one
# variable, so a `vanilla` APK cannot end up named like a main one.
if (-not $Flavor) {
    $FlavorFile = Join-Path $root 'android_app\flavor.properties'
    if (Test-Path $FlavorFile) {
        foreach ($line in (Get-Content $FlavorFile)) {
            if ($line -match '^\s*flavor\s*=\s*(\S*)\s*$') { $Flavor = $Matches[1]; break }
        }
    }
}
$Flavor = $Flavor.Trim()
if ($Flavor -and $Flavor -notmatch '^[A-Za-z0-9]+$') {
    throw "flavor '$Flavor' must be ASCII letters/digits (see BRANCHES.md)"
}
if ($Flavor) {
    $env:HL5_FLAVOR = $Flavor
    Write-Host "HL5_FLAVOR = $Flavor  (content line; see BRANCHES.md)"
} else {
    Remove-Item Env:\HL5_FLAVOR -ErrorAction SilentlyContinue
    Write-Host "content line: main (all content)"
}

# --- toolchain locations ----------------------------------------------------
$Sdk = @(
    $env:ANDROID_HOME
    $env:ANDROID_SDK_ROOT
    "$env:LOCALAPPDATA\Android\Sdk"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $Sdk) { throw 'Android SDK not found. Set $env:ANDROID_HOME.' }

$NdkVersion = '25.1.8937393'
$Ndk = Join-Path $Sdk "ndk\$NdkVersion"
if (-not (Test-Path $Ndk)) {
    $Ndk = Get-ChildItem (Join-Path $Sdk 'ndk') -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if (-not $Ndk) { throw "Android NDK not found under $Sdk\ndk." }
$NdkBin = Join-Path $Ndk 'toolchains\llvm\prebuilt\windows-x86_64\bin'

$env:ANDROID_HOME = $Sdk
$env:ANDROID_SDK_ROOT = $Sdk
$env:ANDROID_NDK_HOME = $Ndk

# AGP 8.7 needs a JDK 17+ to even resolve its own plugin. Whatever `java` is
# first on PATH is not necessarily that (on this host it is a Java 8 JRE), and
# the failure it produces — "Dependency requires at least JVM runtime version
# 11" while configuring the root project — does not name JAVA_HOME, so pick the
# JDK explicitly.
if (-not $env:JAVA_HOME -or -not (Test-Path (Join-Path $env:JAVA_HOME 'bin\javac.exe'))) {
    $jdk = @(
        Get-ChildItem 'C:\Program Files\Eclipse Adoptium' -Directory -ErrorAction SilentlyContinue
        Get-ChildItem 'C:\Program Files\Java' -Directory -ErrorAction SilentlyContinue
        Get-ChildItem 'C:\Program Files\Microsoft' -Directory -Filter 'jdk*' -ErrorAction SilentlyContinue
    ) | Where-Object { Test-Path (Join-Path $_.FullName 'bin\javac.exe') } |
        Sort-Object Name -Descending | Select-Object -First 1
    if (-not $jdk) { throw 'No JDK 17+ found. Set $env:JAVA_HOME to one.' }
    $env:JAVA_HOME = $jdk.FullName
}
Write-Host "JAVA_HOME = $env:JAVA_HOME"

$Adb = @(
    $env:HL5_ADB
    (Join-Path $Sdk 'platform-tools\adb.exe')
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if ($Install -and -not $Adb) { throw 'adb not found. Set $env:HL5_ADB.' }

# --- keystore bootstrap -----------------------------------------------------
if ($NewKeystore) {
    $ks = Join-Path $HOME 'heroes_lore5_release.jks'
    if (Test-Path $ks) { throw "$ks already exists; refusing to overwrite a signing key." }
    $keytool = Join-Path $env:JAVA_HOME 'bin\keytool.exe'
    if (-not (Test-Path $keytool)) { $keytool = 'keytool' }
    & $keytool -genkeypair -v -keystore $ks -alias heroeslore5 -keyalg RSA -keysize 4096 -validity 10000
    if ($LASTEXITCODE -ne 0) { throw "keytool failed ($LASTEXITCODE)" }
    Write-Host ''
    Write-Host "Keystore written to $ks. Add to %USERPROFILE%\.gradle\gradle.properties:"
    Write-Host "  hl5.keystore=$($ks -replace '\\','/')"
    Write-Host '  hl5.keystorePassword=<the store password you just typed>'
    Write-Host '  hl5.keyAlias=heroeslore5'
    Write-Host '  hl5.keyPassword=<the key password you just typed>'
    Write-Host ''
    Write-Host 'KEEP THIS FILE AND THESE PASSWORDS. An app can never be updated on Play with a different key.'
    return
}

# --- dlltool shim -----------------------------------------------------------
# Cross-compiling to android from a windows-gnu host still builds HOST
# build-deps (cc -> windows-sys, getrandom) whose raw-dylib imports rustc
# materialises by shelling out to `dlltool`. The GNU dlltool rustup ships in
# `self-contained` needs an `as` the toolchain does not include, and a renamed
# llvm-dlltool is handed GNU-only flags it rejects. So: a real wrapper.
# `target.<host>.rustflags` does not reach build-deps, which is why this is a
# PATH shim rather than a config setting. See tools/dlltool_shim.rs.
$Shim = Join-Path $env:TEMP 'hl5-dlltool'
New-Item -ItemType Directory -Force $Shim | Out-Null
$RealDlltool = Join-Path $Shim 'real-llvm-dlltool.exe'
$llvmDlltool = "$HOME\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\llvm-dlltool.exe"
if (Test-Path $llvmDlltool) {
    Copy-Item $llvmDlltool $RealDlltool -Force
} elseif (Test-Path "$NdkBin\llvm-ar.exe") {
    # llvm-ar/llvm-lib/llvm-ranlib/llvm-dlltool are ONE multicall binary that
    # picks its personality from argv[0], so a copy named llvm-dlltool IS
    # llvm-dlltool. It loads its DLLs from the NDK bin dir, hence $NdkBin on
    # PATH below.
    Copy-Item "$NdkBin\llvm-ar.exe" $RealDlltool -Force
} else {
    throw "No llvm-dlltool available (looked in the rustup gnu toolchain and $NdkBin)."
}
& "$HOME\.cargo\bin\rustc.exe" -O --edition 2021 (Join-Path $root 'tools\dlltool_shim.rs') -o (Join-Path $Shim 'dlltool.exe')
if ($LASTEXITCODE -ne 0) { throw "could not build the dlltool shim ($LASTEXITCODE)" }
$env:HL5_REAL_DLLTOOL = $RealDlltool
# $NdkBin goes LAST so the NDK's cross tools cannot shadow host ones; it is on
# PATH only so the multicall llvm binary can find its own DLLs.
$env:PATH = "$Shim;$HOME\.cargo\bin;$env:JAVA_HOME\bin;$env:PATH;$NdkBin"

# --- payload ----------------------------------------------------------------
# The guest binary and its asset archive are the licensed game data. They are
# copied from payload/ rather than committed under android_app, so a checkout
# without them fails here — loudly — instead of producing an APK that installs,
# launches, and dies on the first asset read.
$Assets = Join-Path $root 'android_app/app/src/main/assets'
New-Item -ItemType Directory -Force $Assets | Out-Null
foreach ($f in @('libHeroesLore5.so', 'data.vfs.mp3', 'mtouchdevice.txt')) {
    $src = Join-Path $root "payload/$f"
    if (-not (Test-Path $src)) {
        throw "payload/$f is missing. Extract it from the original APK (lib/armeabi/ and assets/)."
    }
    Copy-Item $src (Join-Path $Assets $f) -Force
}

# The launcher icon is the original's and lives with the rest of the payload.
# A checkout without payload/ fails here rather than shipping a placeholder
# nobody would recognise.
$Mipmaps = Join-Path $root 'android_app/app/src/main/res'
foreach ($d in @('ldpi', 'mdpi', 'hdpi')) {
    $src = Join-Path $root "payload/icon-$d.png"
    if (-not (Test-Path $src)) {
        throw "payload/icon-$d.png is missing. Extract res/drawable-$d-v4/icon.png from the original APK."
    }
    $dst = Join-Path $Mipmaps "mipmap-$d"
    New-Item -ItemType Directory -Force $dst | Out-Null
    Copy-Item $src (Join-Path $dst 'ic_launcher.png') -Force
}

# --- rust -------------------------------------------------------------------
$JniLibs = Join-Path $root 'android_app\app\src\main\jniLibs'
if (-not $SkipRust) {
    Push-Location $root
    try {
        # Build one flat argument list. NB: do not name it `$profile` — that is a
        # PowerShell automatic variable (the path to the user's profile script),
        # and splatting it feeds cargo a stray path argument.
        if ($Server) {
            $env:HL5_SERVER = $Server
            Write-Host "HL5_SERVER = $Server"
        } else {
            Remove-Item Env:\HL5_SERVER -ErrorAction SilentlyContinue
        }
        if ($Boot) {
            $env:HL5_BOOT = $Boot
            Write-Host "HL5_BOOT = $Boot"
        } else {
            # Unset means the app's own default: skip-network.
            Remove-Item Env:\HL5_BOOT -ErrorAction SilentlyContinue
        }
        if ($Speed) {
            $env:HL5_SPEED = $Speed
            Write-Host "HL5_SPEED = $Speed"
        } else {
            # Unset means the app's own default: 1.0.
            Remove-Item Env:\HL5_SPEED -ErrorAction SilentlyContinue
        }
        if ($Present) {
            $env:HL5_PRESENT = $Present
            Write-Host "HL5_PRESENT = $Present"
        } else {
            # Unset means the app's own default: egl.
            Remove-Item Env:\HL5_PRESENT -ErrorAction SilentlyContinue
        }
        if ($Affinity) {
            $env:HL5_AFFINITY = $Affinity
            Write-Host "HL5_AFFINITY = $Affinity"
        } else {
            # Unset means the app's own default: prime.
            Remove-Item Env:\HL5_AFFINITY -ErrorAction SilentlyContinue
        }
        if ($Poll) {
            $env:HL5_POLL = $Poll
            Write-Host "HL5_POLL = $Poll"
        } else {
            # Unset means the app's own default: 12 ms.
            Remove-Item Env:\HL5_POLL -ErrorAction SilentlyContinue
        }
        if ($KeepWalking) {
            $env:HL5_KEEP_WALKING = $KeepWalking
            Write-Host "HL5_KEEP_WALKING = $KeepWalking"
        } else {
            # Unset means the app's own default: on.
            Remove-Item Env:\HL5_KEEP_WALKING -ErrorAction SilentlyContinue
        }
        if ($PadHud) {
            $env:HL5_PAD_HUD = $PadHud
            Write-Host "HL5_PAD_HUD = $PadHud"
        } else {
            # Unset means the app's own default: auto.
            Remove-Item Env:\HL5_PAD_HUD -ErrorAction SilentlyContinue
        }
        if ($Rates) {
            $env:HL5_RATES = $Rates
            Write-Host "HL5_RATES = $Rates"
        } else {
            # Unset means the app's own default: on.
            Remove-Item Env:\HL5_RATES -ErrorAction SilentlyContinue
        }
        if ($TickCap) {
            $env:HL5_TICK_CAP = $TickCap
            Write-Host "HL5_TICK_CAP = $TickCap"
        } else {
            # Unset means the app's own default: 24 Hz.
            Remove-Item Env:\HL5_TICK_CAP -ErrorAction SilentlyContinue
        }
        if ($CashItems) {
            $env:HL5_CASH_ITEMS = $CashItems
            Write-Host "HL5_CASH_ITEMS = $CashItems"
        } else {
            # Unset means the app's own default: on.
            Remove-Item Env:\HL5_CASH_ITEMS -ErrorAction SilentlyContinue
        }
        if ($SkillTime) {
            $env:HL5_SKILL_TIME = $SkillTime
            Write-Host "HL5_SKILL_TIME = $SkillTime"
        } else {
            # Unset means the app's own default: on.
            Remove-Item Env:\HL5_SKILL_TIME -ErrorAction SilentlyContinue
        }
        if ($DumpSave) {
            $env:HL5_DUMP_SAVE = $DumpSave
            Write-Host "HL5_DUMP_SAVE = $DumpSave"
        } else {
            # Unset means the app's own default: off.
            Remove-Item Env:\HL5_DUMP_SAVE -ErrorAction SilentlyContinue
        }
        $cargoArgs = @('ndk', '-t', 'arm64-v8a', '--platform', '26', '-o', $JniLibs, 'build')
        if ($Release -or $Fast) { $cargoArgs += '--release' }
        $cargoArgs += @('-p', 'hl5_android')
        Write-Host "cargo $($cargoArgs -join ' ')"
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) { throw "cargo ndk failed ($LASTEXITCODE)" }
    } finally {
        Pop-Location
    }
}
$so = Join-Path $JniLibs 'arm64-v8a\libmain.so'
if (-not (Test-Path $so)) { throw "expected $so to exist after the rust build." }
Write-Host ("libmain.so: {0:N1} MB" -f ((Get-Item $so).Length / 1MB))

# --- version bump (release only) --------------------------------------------
$VersionFile = Join-Path $root 'android_app\version.properties'
$AppVersionCode = $null
$AppVersionName = $null
if ($Release -and (Test-Path $VersionFile)) {
    $lines = Get-Content $VersionFile
    $code = [int](($lines | Where-Object { $_ -match '^versionCode=' }) -replace 'versionCode=', '')
    $code++
    $lines = $lines | ForEach-Object { if ($_ -match '^versionCode=') { "versionCode=$code" } else { $_ } }
    Set-Content $VersionFile $lines -Encoding ascii
    $AppVersionCode = $code
    $AppVersionName = (($lines | Where-Object { $_ -match '^versionName=' }) -replace 'versionName=', '').Trim()
    Write-Host "version -> $AppVersionName (versionCode $AppVersionCode)"
}

# --- gradle -----------------------------------------------------------------
Push-Location (Join-Path $root 'android_app')
try {
    $task = if ($Release) { 'assembleRelease' } else { 'assembleDebug' }
    & .\gradlew.bat $task --console=plain
    if ($LASTEXITCODE -ne 0) { throw "gradle $task failed ($LASTEXITCODE)" }
} finally {
    Pop-Location
}

$outDir = Join-Path $root ('android_app\app\build\outputs\apk\' + $(if ($Release) { 'release' } else { 'debug' }))
$apk = Get-ChildItem $outDir -Filter '*.apk' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $apk) { throw "no APK found in $outDir" }
Write-Host ("APK: {0} ({1:N1} MB)" -f $apk.FullName, ($apk.Length / 1MB))

# --- dist copy (release only) -----------------------------------------------
#
# Gradle always writes `app-release.apk`, which says nothing about what is in
# it. A build that leaves this tree has to carry its version in its name, or
# "which one is on that phone" has no answer a month later. Same rule as the
# sibling hero3/hero4 repos, whose dist/ holds `hero3-v0.1.0-vc11.apk`.
#
# The versionCode is in the name as well as the versionName because this script
# bumps the code on every release build and the name only when a human does:
# without it two different builds would share a filename and the second would
# silently overwrite the first.
if ($Release -and $AppVersionName) {
    $distDir = Join-Path $root 'dist'
    New-Item -ItemType Directory -Force $distDir | Out-Null
    # `hero5[-<flavor>]-v<name>-vc<code>.apk`. The flavor token is not optional
    # on a vanilla build: dist/ has to say which of the two lines a file is.
    $distName = if ($Flavor) {
        "hero5-{0}-v{1}-vc{2}.apk" -f $Flavor, $AppVersionName, $AppVersionCode
    } else {
        "hero5-v{0}-vc{1}.apk" -f $AppVersionName, $AppVersionCode
    }
    $distApk = Join-Path $distDir $distName
    Copy-Item $apk.FullName $distApk -Force
    Write-Host ("dist: {0}" -f $distApk)

    # The signing fingerprint, printed rather than assumed. Every shipped build
    # must have the same one: Android refuses an update signed by a different
    # key, and the only way out for a player is uninstall -- which is their
    # save. dist/README.md records the value this should equal.
    $apksigner = Get-ChildItem (Join-Path $Sdk 'build-tools') -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        ForEach-Object { Join-Path $_.FullName 'apksigner.bat' } |
        Where-Object { Test-Path $_ } |
        Select-Object -First 1
    if ($apksigner) {
        $sha = & $apksigner verify --print-certs $distApk 2>&1 |
            Select-String -Pattern 'SHA-256 digest' | Select-Object -First 1
        if ($sha) { Write-Host ("signer: {0}" -f $sha.ToString().Trim()) }
        else { Write-Host 'signer: APK is not signed (no -Release keystore?)' }
    } else {
        Write-Host 'signer: apksigner not found; fingerprint not checked'
    }
    $apk = Get-Item $distApk
}

if ($Install) {
    $sel = if ($Device) { @('-s', $Device) } else { @() }
    & $Adb @sel install -r $apk.FullName
    if ($LASTEXITCODE -ne 0) { throw "adb install failed ($LASTEXITCODE)" }
    & $Adb @sel shell am force-stop kr.heroeslore.five
    & $Adb @sel logcat -c
    & $Adb @sel shell am start -n 'kr.heroeslore.five/.MainActivity'
    Write-Host 'Installed and launched.'
    if ($Logcat) { & $Adb @sel logcat -s hl5:V AndroidRuntime:E }
    else { Write-Host "Logs: adb $sel logcat -s hl5" }
}
