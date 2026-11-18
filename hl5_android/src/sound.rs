//! Forwarding the guest's audio calls to `Hl5Audio` on the Java side.
//!
//! `hl5_core::audio` records what the guest asked for — a byte range inside the
//! archive, and then play/stop/pause/free against a handle — and queues each
//! call as an [`AudioEvent`]. This drains that queue once a frame and replays
//! it onto `SoundPool` and `MediaPlayer`.
//!
//! # Why the Java class exists
//!
//! Everything here could have been raw JNI: `AudioAttributes.Builder`,
//! `SoundPool.Builder`, `MediaPlayer.setDataSource`. That is a hundred lines of
//! method-signature strings the Rust compiler cannot check, for something
//! `javac` checks for free. So the Android API usage is in
//! `Hl5Audio.java` and this file only calls its seven static methods.
//!
//! # Failing quietly is the right failure
//!
//! Every call here is best-effort. If the class will not resolve, or `openFd`
//! throws because the archive got compressed into the APK, `live` goes false
//! and the game runs silent — which is a far better outcome than a panic on
//! the thread that owns the frame. The reason is always in logcat under `hl5`.

use hl5_core::audio::{AudioEvent, ClipKind};
use jni::objects::{Global, JClass, JObject, LoaderContext};
use jni::{jni_sig, jni_str, Env, JavaVM};
use winit::platform::android::activity::AndroidApp;

/// Must match `Hl5Audio.KIND_SOUND` / `KIND_MUSIC`.
const KIND_SOUND: i32 = 0;
const KIND_MUSIC: i32 = 1;

pub struct Sound {
    vm: JavaVM,
    class: Global<JClass<'static>>,
    /// Cleared the first time a call fails in a way that will not recover, so
    /// a broken seam costs one log line rather than one per frame.
    live: bool,
}

impl Sound {
    /// Resolve `Hl5Audio` and open the archive. `None` means no audio this
    /// run; the caller carries on without it.
    pub fn new(app: &AndroidApp, vfs_name: &str) -> Option<Sound> {
        let vm_ptr = app.vm_as_ptr() as *mut jni::sys::JavaVM;
        let activity_ptr = app.activity_as_ptr() as jni::sys::jobject;
        if vm_ptr.is_null() || activity_ptr.is_null() {
            log::warn!("audio: no JavaVM/activity pointer; running silent");
            return None;
        }
        // SAFETY: android-activity owns both for the life of the process, and
        // this runs on the thread it created for `android_main`.
        let vm = unsafe { JavaVM::from_raw(vm_ptr) };

        let name = vfs_name.to_string();
        let r: Result<Global<JClass<'static>>, jni::errors::Error> =
            vm.attach_current_thread(|env: &mut Env| {
                // SAFETY: the activity reference outlives this scope.
                let activity = unsafe { JObject::from_raw(env, activity_ptr) };

                // Resolve through the *activity's* class loader rather than
                // plain `FindClass`. This thread was created by native code,
                // so the JNI class search starts from the system loader, which
                // cannot see classes from the APK. Going through the activity
                // is the reliable route and costs one call at startup.
                let class = LoaderContext::FromObject(&activity).find_class(
                    env,
                    jni_str!("kr/heroeslore/five/Hl5Audio"),
                    false,
                )?;
                let class = env.new_global_ref(&class)?;

                let assets = env
                    .call_method(
                        &activity,
                        jni_str!("getAssets"),
                        jni_sig!(() -> android.content.res.AssetManager),
                        &[],
                    )?
                    .l()?;
                let jname = env.new_string(&name)?;
                let ok = env
                    .call_static_method(
                        &class,
                        jni_str!("init"),
                        jni_sig!(
                            (ctx: android.content.Context,
                             assets: android.content.res.AssetManager,
                             name: java.lang.String) -> jboolean
                        ),
                        &[(&activity).into(), (&assets).into(), (&jname).into()],
                    )?
                    .z()?;
                if !ok {
                    return Err(jni::errors::Error::NullPtr("Hl5Audio.init returned false"));
                }
                Ok(class)
            });

        match r {
            Ok(class) => Some(Sound { vm, class, live: true }),
            Err(e) => {
                log::warn!("audio: setup failed ({e}); running silent");
                None
            }
        }
    }

    /// Replay one frame's worth of guest audio calls, in order.
    pub fn apply(&mut self, events: &[AudioEvent]) {
        if !self.live || events.is_empty() {
            return;
        }
        let class = &self.class;
        let r: Result<(), jni::errors::Error> = self.vm.attach_current_thread(|env: &mut Env| {
            for ev in events {
                match *ev {
                    AudioEvent::Load { key, kind, offset, length } => {
                        // `Pcm` never reaches here (`audio.rs` does not queue
                        // it), but the match has to be total and silently
                        // mapping it onto a sound would play the wrong bytes.
                        let k = match kind {
                            ClipKind::Sound => KIND_SOUND,
                            ClipKind::Music => KIND_MUSIC,
                            ClipKind::Pcm => continue,
                        };
                        env.call_static_method(
                            class,
                            jni_str!("load"),
                            jni_sig!(
                                (key: jlong, kind: jint, offset: jlong, length: jlong) -> void
                            ),
                            &[
                                (key as i64).into(),
                                k.into(),
                                (offset as i64).into(),
                                (length as i64).into(),
                            ],
                        )?;
                    }
                    AudioEvent::Play { key, looping } => {
                        env.call_static_method(
                            class,
                            jni_str!("play"),
                            jni_sig!((key: jlong, looping: jboolean) -> void),
                            &[(key as i64).into(), looping.into()],
                        )?;
                    }
                    AudioEvent::Stop { key } => one(env, class, jni_str!("stop"), key)?,
                    AudioEvent::Pause { key } => one(env, class, jni_str!("pause"), key)?,
                    AudioEvent::Resume { key } => one(env, class, jni_str!("resume"), key)?,
                    AudioEvent::Free { key } => one(env, class, jni_str!("free"), key)?,
                    AudioEvent::Volume { master, sound, music } => {
                        env.call_static_method(
                            class,
                            jni_str!("volume"),
                            jni_sig!((master: jint, sound: jint, music: jint) -> void),
                            &[master.into(), sound.into(), music.into()],
                        )?;
                    }
                }
            }
            Ok(())
        });
        if let Err(e) = r {
            log::warn!("audio: call failed ({e}); silencing the seam for this run");
            self.live = false;
        }
    }

    /// Silence everything for `onPause`, remembering what was playing.
    ///
    /// Without this the music keeps going over whatever the user switched to:
    /// the guest is stopped at the same moment, so nothing on its side ever
    /// asks for the pause.
    pub fn suspend(&mut self) {
        self.no_args(jni_str!("suspend"));
    }

    /// The other half of [`Sound::suspend`], for `onResume`.
    pub fn restore(&mut self) {
        self.no_args(jni_str!("restore"));
    }

    fn no_args(&mut self, method: &'static jni::strings::JNIStr) {
        if !self.live {
            return;
        }
        let class = &self.class;
        let r: Result<(), jni::errors::Error> = self.vm.attach_current_thread(|env: &mut Env| {
            env.call_static_method(class, method, jni_sig!(() -> void), &[])?;
            Ok(())
        });
        if let Err(e) = r {
            log::warn!("audio: {method:?} failed ({e})");
        }
    }
}

/// The four `(long) -> void` methods, which are otherwise identical.
fn one(
    env: &mut Env,
    class: &Global<JClass<'static>>,
    method: &'static jni::strings::JNIStr,
    key: u64,
) -> Result<(), jni::errors::Error> {
    env.call_static_method(
        class,
        method,
        jni_sig!((key: jlong) -> void),
        &[(key as i64).into()],
    )?;
    Ok(())
}
