//! The `CletEntry` static methods the guest calls back into Java through.
//!
//! `nativeInitKernel` looks up 55 of these; the guest calls a subset of them
//! as it boots and runs. Each one below is the host standing in for a Java
//! method whose *behaviour* was read from `classes.dex`, not guessed:
//!
//! * `getAssetSize`/`loadAsset` — `MidasAssetManager` opens the named file in
//!   the APK's `assets/` (`AssetManager.open(name).available()` and a read),
//!   so the host serves them from `Config::assets`. These are individual
//!   asset files, *not* the VFS archive — that goes through `nativeInitVFS`
//!   and the guest's own `MIDASKernelManager::loadAssetFromVFS`.
//! * `getUniqueNumberFromStrings`/`getUniqueAssetNameFromID` — a plain
//!   sequential bijection in Java (`id = table.size()+1`, both directions
//!   stored), so the host reproduces it exactly with an index list.
//! * device/memory/network probes — constants that describe the target
//!   device (`Config::model`, plenty of memory, network up).
//! * audio clips — recorded with the byte range the guest named, and with
//!   honest play/stop/pause state. The decoding belongs to the platform in
//!   the original too (`SoundPool.load(fd, offset, length, 1)`), so that half
//!   is the Android shell's. See `audio.rs`.
//! * `initMTX`/`requestMTX` — the cash-shop billing seam. Documented in
//!   `docs/SERVER.md`; here they succeed without contacting a carrier so the
//!   in-game flow proceeds.
//!
//! Return values are placed by the JNI `CallStatic*` slot in `jni.rs`; this
//! module only computes them. A `long` return travels in the low/high halves
//! of the returned `u64`; an object return carries a guest handle.

use crate::audio::{Clip, ClipKind, ClipState, Pcm};
use crate::fmt::VaSource;
use crate::Vm;

/// What a `CletEntry` callback produced. The JNI dispatcher maps this onto
/// the register(s) the calling `CallStatic*Method` slot expects.
pub enum CletRet {
    /// `int`/`boolean`/`void` (void ignores the value).
    Word(u32),
    /// `long` — returned in r0:r1.
    Long(u64),
    /// An object handle (jstring), or 0 for null.
    Object(u32),
}

impl Vm {
    /// Serve one `CletEntry` static-method call. `method` is the part after
    /// `CletEntry.` — e.g. `getAssetSize(Ljava/lang/String;)I`. Arguments are
    /// pulled from `va` in signature order, exactly as the JNI varargs
    /// wrapper laid them out.
    pub(crate) fn clet_call(&mut self, method: &str, va: &mut VaSource) -> Result<CletRet, String> {
        // Split "name(sig)ret" into just the name for matching.
        let name = method.split('(').next().unwrap_or(method);

        Ok(match name {
            // ---- assets: individual files under Config::assets -------------
            "getAssetSize" => {
                let jstr = va.next_obj(self);
                let asset = self.jni_string(jstr);
                CletRet::Word(self.asset_size(&asset))
            }
            "loadAsset" => {
                let id = va.next_word(self);
                let arr = va.next_obj(self);
                let len = va.next_word(self);
                let name = self.asset_name_of(id);
                let n = match name {
                    Some(n) => self.asset_read_into(&n, arr, len),
                    None => u32::MAX,
                };
                CletRet::Word(n)
            }
            "getUniqueNumberFromStrings" => {
                let jstr = va.next_obj(self);
                let s = self.jni_string(jstr);
                CletRet::Word(self.asset_uid(&s))
            }
            "getUniqueAssetNameFromID" => {
                let id = va.next_word(self);
                let name = self.asset_name_of(id).unwrap_or_default();
                let h = self.new_string_utf(name.as_bytes());
                CletRet::Object(h)
            }

            // ---- device / environment -------------------------------------
            "getModel" => {
                let h = self.new_string_utf(self.cfg.model.clone().as_bytes());
                CletRet::Object(h)
            }
            "getManufacturer" => CletRet::Object(self.new_string_utf(b"samsung")),
            "getOpenGLESVersion" => CletRet::Object(self.new_string_utf(b"OpenGL ES-CM 1.1")),
            "getIMEI" => CletRet::Object(self.new_string_utf(b"000000000000000")),
            "getIMSI" => CletRet::Object(self.new_string_utf(b"450000000000000")),
            // `MC_knlGetSystemProperty("PHONENUMBER")` resolves to this
            // (0x14e118), and `PacketWrite_Login` puts it on the wire — so
            // this string *is* the account the private server sees.
            "getMin" => {
                let id = self.cfg.account_id.clone();
                CletRet::Object(self.new_string_utf(id.as_bytes()))
            }
            "getOSVersion" => CletRet::Word(29),
            "isFBOSupported" => CletRet::Word(1),

            // ---- memory (bytes) — report a roomy device -------------------
            // Roomy, but it MUST stay below 2^31. These are declared `()J` and
            // returned by `CallStaticLongMethod`, yet the guest reads only the
            // low word and compares it *signed*: `MC_fsOpen` (0x14d718) does
            //
            //     bl  MIDASKernelManager::getAvailableStorageSpace
            //     cmp r0, #0xa
            //     mvnle r0, #0xc      ; -13
            //     ble  <return>       ; ...without ever calling open()
            //
            // At exactly 2 GB the low word is 0x80000000, which is negative as
            // an i32, so every write-mode open returned -13 and the whole host
            // filesystem was unreachable — the guest still said "저장되었습니다"
            // because nothing above `MC_fsOpen` checks its result. That is why
            // a completed playthrough left 34 directories and 0 files.
            "getAvailableInternalMemorySize" | "getTotalInternalMemorySize"
            | "getAvailableExternalMemorySize" | "getTotalExternalMemorySize" => {
                CletRet::Long(1024 * 1024 * 1024) // 1 GB — low word 0x40000000
            }
            "totalMemory" | "freeMemory" => CletRet::Word(512 * 1024 * 1024),

            // ---- network reachability -------------------------------------
            // Report "up" so the guest attempts to connect; the socket layer
            // then decides real-server vs. dead-server (see files.rs / SERVER.md).
            "isNetworkAvailable" => CletRet::Word(1),
            "isAirplaneModeOn" => CletRet::Word(0),
            "isNetworkRoaming" => CletRet::Word(0),

            // ---- audio ----------------------------------------------------
            //
            // The clips live in the archive as Ogg Vorbis and the original
            // never decodes them itself either: `MidasSoundManager.load` hands
            // `SoundPool.load(vfsfd, vfsOffset + offset, length, 1)` a byte
            // range and lets the platform do the work. So the host records the
            // range and the play state, and the Android shell forwards both
            // unchanged. See `audio.rs`.
            //
            //   putSoundClip(long key, long offset, long length, int type)
            //   putMusicClip(long key, long offset, long length)
            //   putClip(long key, int, int, byte[] pcm, int, int, int, int, int)
            "putSoundClip" | "putMusicClip" => {
                let key = va.next_long(self);
                let offset = va.next_long(self);
                let length = va.next_long(self);
                let hint = if name == "putSoundClip" { va.next_word(self) as i32 } else { 0 };
                let kind =
                    if name == "putSoundClip" { ClipKind::Sound } else { ClipKind::Music };
                let clip = Clip {
                    kind,
                    state: ClipState::Stopped,
                    looping: false,
                    offset,
                    length,
                    hint,
                    pcm: None,
                    plays: 0,
                };
                CletRet::Word(self.audio.put(key, clip))
            }
            "putClip" => {
                let key = va.next_long(self);
                let _a = va.next_word(self);
                let _b = va.next_word(self);
                let arr = va.next_obj(self);
                let off = va.next_word(self);
                let len = va.next_word(self);
                let bits = va.next_word(self) as i32;
                let rate = va.next_word(self) as i32;
                let channels = va.next_word(self) as i32;
                // Array handles are guest heap addresses (see `jni.rs`), so the
                // samples are simply there to be copied.
                let data = self
                    .mem
                    .slice(arr.wrapping_add(off), len)
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default();
                let clip = Clip {
                    kind: ClipKind::Pcm,
                    state: ClipState::Stopped,
                    looping: false,
                    offset: 0,
                    length: data.len() as u64,
                    hint: 0,
                    pcm: Some(Pcm { data, bits, rate, channels }),
                    plays: 0,
                };
                CletRet::Word(self.audio.put(key, clip))
            }
            "clipPlay" | "clipPlaySound" | "clipPlayMusic" => {
                let key = va.next_long(self);
                let looping = va.next_word(self) != 0;
                CletRet::Word(self.audio.play(key, looping))
            }
            "clipStop" | "clipStopSound" | "clipStopMusic" => {
                let key = va.next_long(self);
                CletRet::Word(self.audio.stop(key))
            }
            "clipPause" | "clipPauseSound" | "clipPauseMusic" => {
                let key = va.next_long(self);
                CletRet::Word(self.audio.pause(key))
            }
            "clipResume" | "clipResumeSound" | "clipResumeMusic" => {
                let key = va.next_long(self);
                CletRet::Word(self.audio.resume(key))
            }
            "clipFree" | "clipFreeSound" | "clipFreeMusic" => {
                let key = va.next_long(self);
                CletRet::Word(self.audio.free(key))
            }
            "getClipStatus" => {
                let key = va.next_long(self);
                CletRet::Word(self.audio.status(key))
            }
            "setVolume" => {
                let v = va.next_word(self) as i32;
                self.audio.set_volume(v);
                CletRet::Word(0)
            }
            "setSoundVolume" => {
                let v = va.next_word(self) as i32;
                self.audio.set_sound_volume(v);
                CletRet::Word(0)
            }
            "setMusicVolume" => {
                let v = va.next_word(self) as i32;
                self.audio.set_music_volume(v);
                CletRet::Word(0)
            }
            "getVolume" => CletRet::Word(self.audio.volume as u32),
            "getSoundVolume" => CletRet::Word(self.audio.sound_volume as u32),
            "getMusicVolume" => CletRet::Word(self.audio.music_volume as u32),

            // ---- billing seam (cash shop) ---------------------------------
            // A real purchase drives the carrier IAP; offline it succeeds so
            // the shop UI advances. See docs/SERVER.md §MTX.
            "initMTX" => CletRet::Word(0),
            // The carrier IAP is a conversation between the handset and the
            // carrier, so a private server cannot reproduce it (docs/SERVER.md
            // 6.6). Succeeding here is only half of that: `requestMTX` starting
            // the purchase is not what the guest waits on -- it waits for the
            // *result callback*, and until that arrives `bProgress` stays set
            // and the confirmation dialog will not close. So arm the callback
            // and let the next frame deliver it.
            "requestMTX" => {
                self.pending_mtx_result = true;
                CletRet::Word(0)
            }

            // ---- side-effecting voids / misc ------------------------------
            "exitApp" => {
                self.exit_requested = true;
                CletRet::Word(0)
            }
            "vibration" | "phnCallPlace" | "loadWebpage" => CletRet::Word(0),

            // Everything with a `V` return and no side effect the host cares
            // about: acknowledge. Everything else with a value: 0 is the safe
            // "nothing happened" answer for these probes.
            "attachBufferIDtoResourceID" | "detachBufferIDfromResourceID" => CletRet::Word(0),

            other => {
                return Err(format!(
                    "CletEntry callback `{other}` is looked up but not implemented \
                     (called as {method})"
                ));
            }
        })
    }

    // ---- asset helpers ------------------------------------------------------

    fn asset_path(&self, name: &str) -> std::path::PathBuf {
        // Asset names are ASCII resource paths; keep them under Config::assets.
        let mut p = self.cfg.assets.clone();
        for c in name.split('/') {
            if !matches!(c, "" | "." | "..") {
                p.push(c);
            }
        }
        p
    }

    fn asset_size(&self, name: &str) -> u32 {
        match std::fs::metadata(self.asset_path(name)) {
            Ok(m) => m.len() as u32,
            Err(_) => u32::MAX, // Java returns -1 on IOException
        }
    }

    fn asset_read_into(&mut self, name: &str, arr: u32, len: u32) -> u32 {
        let data = match std::fs::read(self.asset_path(name)) {
            Ok(d) => d,
            Err(_) => return u32::MAX,
        };
        let n = (data.len() as u32).min(len);
        self.mem.write_bytes(arr, &data[..n as usize]);
        n
    }

    /// The Java bijection: first sight of a name assigns `size()+1`.
    fn asset_uid(&mut self, name: &str) -> u32 {
        if let Some(i) = self.asset_ids.iter().position(|n| n == name) {
            return i as u32 + 1;
        }
        self.asset_ids.push(name.to_string());
        self.asset_ids.len() as u32
    }

    fn asset_name_of(&self, id: u32) -> Option<String> {
        if id == 0 {
            return None;
        }
        self.asset_ids.get((id - 1) as usize).cloned()
    }
}
