//! The audio seam: what the guest asked to be played, and where its bytes are.
//!
//! Nothing here decodes or mixes, and that is not a shortcut — it is what the
//! original does. `MidasSoundManager.load` (in `classes.dex`) hands
//! `SoundPool.load(vfsfd, vfsOffset + offset, length, 1)` a **byte range of
//! the APK asset**, and `MidasMusicManager` gives `MediaPlayer` the same three
//! numbers. The clips inside that archive are Ogg Vorbis — the guest asks for
//! `c/snd/eff_00.ogg` and friends, and pulling one out at the offset this
//! layer records starts with `OggS`. So the platform decoder *is* the intended
//! decoder, and writing a Vorbis decoder in Rust would be reimplementing
//! something the target device already has.
//!
//! What the host owes the guest is the other half: a handle table with honest
//! play/pause/stop/free semantics, and the exact byte range, which the Android
//! shell forwards to SoundPool/MediaPlayer unchanged. Headless, that byte
//! range is also the thing that can be *checked* — `hl5_probe --dump-audio`
//! writes every registered clip out, and a valid Ogg file on the other end is
//! proof the seam computed the right numbers.
//!
//! Clips are keyed by the guest's own handle, which is what Java does
//! (`mSoundPoolMap.get(Long.valueOf(key))`). The value `put*Clip` returns is a
//! separate thing — SoundPool's own sound id, where **0 means the load
//! failed** and anything positive is success.

use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipKind {
    /// `putSoundClip` — a short effect, `SoundPool` on the device.
    Sound,
    /// `putMusicClip` — background music, `MediaPlayer` on the device.
    Music,
    /// `putClip` — raw PCM handed over in a `byte[]` rather than named in the
    /// archive. This guest has not been observed using it.
    Pcm,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Debug)]
pub struct Clip {
    pub kind: ClipKind,
    pub state: ClipState,
    pub looping: bool,
    /// Byte range inside `Config::vfs`, for `Sound` and `Music`.
    pub offset: u64,
    pub length: u64,
    /// `putSoundClip`'s trailing `int`. `putMusicClip` has no such argument.
    pub hint: i32,
    /// Samples for a `Pcm` clip, copied out of the guest's `byte[]`.
    pub pcm: Option<Pcm>,
    /// How many times the guest has started this clip. Diagnostics only, but
    /// it is the cheapest way to see whether a run ever asked for sound.
    pub plays: u32,
}

#[derive(Clone, Debug)]
pub struct Pcm {
    pub data: Vec<u8>,
    pub bits: i32,
    pub rate: i32,
    pub channels: i32,
}

/// One thing the guest asked the platform to do, in the order it asked.
///
/// The clip table above is *state* — what exists and what is playing — and a
/// shell that only read the state could not tell "started again" from "still
/// going", which is the difference between a sound effect firing twice and
/// firing once. So the calls are queued as they arrive and the shell replays
/// them onto SoundPool/MediaPlayer verbatim.
///
/// The queue is unbounded on purpose: it is drained every frame on the device,
/// and headless it is bounded by how many audio calls the replay makes (a few
/// hundred). Dropping the oldest to cap it would lose exactly the events a
/// stalled shell most needs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AudioEvent {
    /// A byte range inside the VFS archive became a clip. `Pcm` clips are not
    /// reported: nothing observed uses them, and there is no byte range to
    /// hand the platform.
    Load { key: u64, kind: ClipKind, offset: u64, length: u64 },
    Play { key: u64, looping: bool },
    Stop { key: u64 },
    Pause { key: u64 },
    Resume { key: u64 },
    Free { key: u64 },
    /// 0..100, the WIPI scale. Emitted on every set, because the guest lowers
    /// music under a dialogue and raises it again afterwards.
    Volume { master: i32, sound: i32, music: i32 },
}

pub struct Audio {
    clips: BTreeMap<u64, Clip>,
    /// SoundPool ids are positive; 0 is its failure value, so this starts at 1.
    next_id: u32,
    events: Vec<AudioEvent>,
    /// WIPI volumes, 0..100. The guest sets and reads these back.
    pub volume: i32,
    pub sound_volume: i32,
    pub music_volume: i32,
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

impl Audio {
    pub fn new() -> Self {
        Audio {
            clips: BTreeMap::new(),
            next_id: 1,
            events: Vec::new(),
            volume: 100,
            sound_volume: 100,
            music_volume: 100,
        }
    }

    /// Take everything queued since the last call. The shell does this once a
    /// frame; a host with no audio never calls it and the queue simply holds
    /// what the guest asked for.
    pub fn take_events(&mut self) -> Vec<AudioEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn set_volume(&mut self, v: i32) {
        self.volume = v;
        self.push_volume();
    }

    pub fn set_sound_volume(&mut self, v: i32) {
        self.sound_volume = v;
        self.push_volume();
    }

    pub fn set_music_volume(&mut self, v: i32) {
        self.music_volume = v;
        self.push_volume();
    }

    fn push_volume(&mut self) {
        self.events.push(AudioEvent::Volume {
            master: self.volume,
            sound: self.sound_volume,
            music: self.music_volume,
        });
    }

    /// Register a clip under the guest's handle. Returns the id to hand back,
    /// which is non-zero exactly when the load would have succeeded.
    pub fn put(&mut self, key: u64, clip: Clip) -> u32 {
        // A zero-length range is what SoundPool fails on, and saying so is
        // more useful than pretending: the guest retries five times and then
        // logs, which is visible behaviour rather than silence.
        if clip.pcm.is_none() && clip.length == 0 {
            return 0;
        }
        let id = self.next_id;
        self.next_id += 1;
        if clip.pcm.is_none() {
            self.events.push(AudioEvent::Load {
                key,
                kind: clip.kind,
                offset: clip.offset,
                length: clip.length,
            });
        }
        self.clips.insert(key, clip);
        id
    }

    /// Start a clip. 0 on success, -1 if nothing is loaded under that handle —
    /// which is exactly what `MidasSoundManager.play` returns.
    pub fn play(&mut self, key: u64, looping: bool) -> u32 {
        match self.clips.get_mut(&key) {
            Some(c) => {
                c.state = ClipState::Playing;
                c.looping = looping;
                c.plays += 1;
                self.events.push(AudioEvent::Play { key, looping });
                0
            }
            None => u32::MAX,
        }
    }

    pub fn stop(&mut self, key: u64) -> u32 {
        if let Some(c) = self.clips.get_mut(&key) {
            c.state = ClipState::Stopped;
            self.events.push(AudioEvent::Stop { key });
        }
        0
    }

    pub fn pause(&mut self, key: u64) -> u32 {
        if let Some(c) = self.clips.get_mut(&key) {
            if c.state == ClipState::Playing {
                c.state = ClipState::Paused;
                self.events.push(AudioEvent::Pause { key });
            }
        }
        0
    }

    pub fn resume(&mut self, key: u64) -> u32 {
        if let Some(c) = self.clips.get_mut(&key) {
            if c.state == ClipState::Paused {
                c.state = ClipState::Playing;
                self.events.push(AudioEvent::Resume { key });
            }
        }
        0
    }

    pub fn free(&mut self, key: u64) -> u32 {
        if self.clips.remove(&key).is_some() {
            self.events.push(AudioEvent::Free { key });
        }
        0
    }

    /// `CletEntry.getClipStatus`, as `WIPIConstant.getWIPIMediaStatus` encodes
    /// it: -1 when there is no such clip, 2 while it plays, 0 otherwise.
    pub fn status(&self, key: u64) -> u32 {
        match self.clips.get(&key) {
            None => u32::MAX,
            Some(c) if c.state == ClipState::Playing => 2,
            Some(_) => 0,
        }
    }

    pub fn get(&self, key: u64) -> Option<&Clip> {
        self.clips.get(&key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u64, &Clip)> {
        self.clips.iter()
    }

    pub fn len(&self) -> usize {
        self.clips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }
}
