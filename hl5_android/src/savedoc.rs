//! The phone's document screens for the 세이브 내보내기 / 불러오기 rows
//! (`hl5_core::savexfer`): this asks `Hl5Save.java` to open them, and the
//! answer comes back through `nativeDocResult` in `lib.rs`.
//!
//! Like [`crate::sound`], the Android API lives in Java where `javac` checks
//! it, and this only makes three static calls. A seam that will not resolve
//! leaves the rows answering "failed" instead of taking the game down.

use hl5_core::savexfer::DocRequest;
use jni::objects::{Global, JClass, JObject, LoaderContext};
use jni::{jni_sig, jni_str, Env, JavaVM};
use winit::platform::android::activity::AndroidApp;

pub struct SaveDoc {
    vm: JavaVM,
    class: Global<JClass<'static>>,
    /// The phone's offset from UTC when the class was resolved.
    pub utc_offset_min: i32,
}

impl SaveDoc {
    pub fn new(app: &AndroidApp) -> Option<SaveDoc> {
        let vm_ptr = app.vm_as_ptr() as *mut jni::sys::JavaVM;
        let activity_ptr = app.activity_as_ptr() as jni::sys::jobject;
        if vm_ptr.is_null() || activity_ptr.is_null() {
            log::warn!("savexfer: no JavaVM/activity pointer; the save rows will report failure");
            return None;
        }
        // SAFETY: android-activity owns both for the life of the process.
        let vm = unsafe { JavaVM::from_raw(vm_ptr) };
        let r: Result<(Global<JClass<'static>>, i32), jni::errors::Error> =
            vm.attach_current_thread(|env: &mut Env| {
                // SAFETY: the activity reference outlives this scope.
                let activity = unsafe { JObject::from_raw(env, activity_ptr) };
                // Through the activity's loader: this thread was made by
                // native code and the system loader cannot see the APK.
                let class = LoaderContext::FromObject(&activity).find_class(
                    env,
                    jni_str!("kr/heroeslore/five/Hl5Save"),
                    false,
                )?;
                let class = env.new_global_ref(&class)?;
                let offset = env
                    .call_static_method(&class, jni_str!("utcOffsetMinutes"), jni_sig!(() -> jint), &[])?
                    .i()?;
                Ok((class, offset))
            });
        match r {
            Ok((class, utc_offset_min)) => Some(SaveDoc { vm, class, utc_offset_min }),
            Err(e) => {
                log::warn!("savexfer: Hl5Save unavailable ({e}); the save rows will report failure");
                None
            }
        }
    }

    /// Open the screen `req` asks for. False when the call did not reach
    /// Java; the caller then answers the request as failed itself.
    pub fn request(&self, req: &DocRequest) -> bool {
        let class = &self.class;
        let r: Result<bool, jni::errors::Error> = self.vm.attach_current_thread(|env: &mut Env| {
            match req {
                DocRequest::Export { file, name } => {
                    let path = env.new_string(file.to_string_lossy())?;
                    let name = env.new_string(name)?;
                    env.call_static_method(
                        class,
                        jni_str!("export"),
                        jni_sig!((path: java.lang.String, name: java.lang.String) -> jboolean),
                        &[(&path).into(), (&name).into()],
                    )?
                    .z()
                }
                DocRequest::Import { file } => {
                    let path = env.new_string(file.to_string_lossy())?;
                    env.call_static_method(
                        class,
                        jni_str!("pick"),
                        jni_sig!((path: java.lang.String) -> jboolean),
                        &[(&path).into()],
                    )?
                    .z()
                }
            }
        });
        match r {
            Ok(ok) => ok,
            Err(e) => {
                log::warn!("savexfer: Hl5Save call failed ({e})");
                false
            }
        }
    }
}
