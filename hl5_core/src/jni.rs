//! The synthetic JNIEnv / JavaVM the guest is handed.
//!
//! Real JNI is two C structs of function pointers. The guest only ever does
//! two things with them: read the table pointer at offset 0, and call through
//! a slot. So the whole implementation is data: an object word pointing at a
//! table whose every entry is an HLE trampoline ([`crate::hle::TRAMP_JNIENV`]
//! upwards). A call through any slot lands in `Vm::dispatch`, which — until
//! that slot is implemented — dies naming the slot, via [`SLOT_NAMES`], and
//! the caller. That error message *is* the JNI work list generator: run
//! `nativeInitKernel`, read which slot it wanted, implement it, repeat.
//!
//! The table is 233 entries because that is JNI 1.6's layout: 4 reserved
//! words, then GetVersion at index 4 through GetObjectRefType at index 232 —
//! the same count CLAUDE.md records for this guest.

use crate::hle;
use crate::mem::JNI_BASE;
use crate::Vm;

/// Guest address of the `JNIEnv` (a pointer to the function table).
pub const ENV_ADDR: u32 = JNI_BASE;
const ENV_FNS: u32 = JNI_BASE + 0x10;

/// Guest address of the `JavaVM`.
pub const VM_ADDR: u32 = JNI_BASE + 0x800;
const VM_FNS: u32 = JNI_BASE + 0x810;

pub const ENV_SLOTS: usize = 233;
const VM_SLOTS: usize = 8;

/// First jclass/jmethodID/jobject handle value. Handles are opaque to the
/// guest; these sit in the mapped-but-empty part of the JNI window so a guest
/// that wrongly dereferences one reads zeros instead of faulting at random.
const HANDLE_BASE: u32 = JNI_BASE + 0x0004_0000;

/// What the guest has looked up so far. A handle *is* its description — the
/// registry's job during bring-up is to turn "CallStaticVoidMethod on
/// jmethodID 0x1854_0008" into "CletEntry.flipBuffer([II)V" in an error.
#[derive(Default)]
pub struct JniState {
    names: Vec<String>,
    /// jbyteArray / jstring handle -> element length in bytes. The handle
    /// itself is the guest address of the backing buffer, so
    /// `GetByteArrayElements`/`GetStringUTFChars` just return the handle and
    /// this map answers `GetArrayLength`/`GetStringUTFLength`.
    lengths: std::collections::HashMap<u32, u32>,
    /// Object handle -> its single `int` field value. Used for the one Java
    /// object the guest inspects: a `FileDescriptor` whose `descriptor` field
    /// is the VFS file descriptor (`nativeInitVFS` reads it via `GetIntField`).
    int_fields: std::collections::HashMap<u32, i32>,
}

impl JniState {
    fn intern(&mut self, name: String) -> u32 {
        let i = match self.names.iter().position(|n| *n == name) {
            Some(i) => i,
            None => {
                self.names.push(name);
                self.names.len() - 1
            }
        };
        HANDLE_BASE + (i as u32) * 4
    }

    /// Everything the guest has looked up, in lookup order. `nativeInitKernel`
    /// populates this with the 19 `CletEntry` callbacks — running it *is* the
    /// measurement of which callbacks exist and with which signatures.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn describe(&self, handle: u32) -> String {
        if handle < HANDLE_BASE || (handle - HANDLE_BASE) % 4 != 0 {
            return format!("{handle:#010x}");
        }
        match self.names.get(((handle - HANDLE_BASE) / 4) as usize) {
            Some(n) => n.clone(),
            None => format!("{handle:#010x}"),
        }
    }
}

impl Vm {
    /// Lay the JNIEnv and JavaVM objects into the JNI window. Called once
    /// from `Vm::new`; every slot points at its own trampoline so a call
    /// through slot `i` reaches `dispatch(TRAMP_JNIENV + i)`.
    pub(crate) fn build_jni(&mut self) {
        self.mem.write_u32(ENV_ADDR, ENV_FNS);
        for i in 0..ENV_SLOTS {
            self.mem
                .write_u32(ENV_FNS + i as u32 * 4, hle::tramp_addr(hle::TRAMP_JNIENV + i));
        }
        self.mem.write_u32(VM_ADDR, VM_FNS);
        for i in 0..VM_SLOTS {
            self.mem
                .write_u32(VM_FNS + i as u32 * 4, hle::tramp_addr(hle::TRAMP_JAVAVM + i));
        }
    }

    /// The `JNIEnv*` to pass as the first argument of any `Java_*` entry point.
    pub fn jni_env(&self) -> u32 {
        ENV_ADDR
    }

    /// Allocate a jstring: a guest buffer holding `bytes` + a NUL. The handle
    /// is the buffer address, so `GetStringUTFChars` is the identity.
    pub(crate) fn new_string_utf(&mut self, bytes: &[u8]) -> u32 {
        let p = self.heap.alloc(bytes.len() as u32 + 1).unwrap_or(0);
        if p != 0 {
            self.mem.write_bytes(p, bytes);
            self.mem.write_bytes(p + bytes.len() as u32, &[0]);
            self.jni.lengths.insert(p, bytes.len() as u32);
        }
        p
    }

    /// Allocate a zeroed jbyteArray of `len` bytes; the handle is its address.
    fn new_byte_array(&mut self, len: u32) -> u32 {
        let p = self.heap.alloc(len.max(1)).unwrap_or(0);
        if p != 0 {
            if let Some(b) = self.mem.slice_mut(p, len) {
                b.fill(0);
            }
            self.jni.lengths.insert(p, len);
        }
        p
    }

    /// Read a jstring handle back to its UTF-8 bytes (guest text is ASCII
    /// here — asset names — so a lossy decode is exact).
    pub(crate) fn jni_string(&self, handle: u32) -> String {
        if handle == 0 {
            return String::new();
        }
        self.mem.cstr(handle)
    }

    /// Make a Java object carrying one `int` field (a `FileDescriptor` whose
    /// `descriptor` is `val`). The handle interns through the same registry
    /// as class/method handles.
    pub(crate) fn new_int_object(&mut self, label: &str, val: i32) -> u32 {
        let h = self.jni.intern(format!("{label}@{val}"));
        self.jni.int_fields.insert(h, val);
        h
    }

    /// Serve a call through a JNIEnv/JavaVM slot. Only the slots that hand
    /// back the synthetic objects themselves are implemented so far; anything
    /// else dies naming the slot — that error is the work list for the JNI
    /// increment, exactly as `--todo` was for the imports.
    pub(crate) fn dispatch_jni(&mut self, id: usize) -> Result<(), String> {
        use crate::hle::{TRAMP_JAVAVM, TRAMP_JNIENV};
        use arm32_cpu::{reg, Mode};

        let (kind, slot, what) = if id >= TRAMP_JAVAVM {
            let s = id - TRAMP_JAVAVM;
            ("JavaVM", s, vm_slot_name(s))
        } else {
            let s = id - TRAMP_JNIENV;
            ("JNIEnv", s, env_slot_name(s))
        };

        self.hle_calls += 1;
        if self.trace {
            let lr = self.cpu.reg_get(Mode::User, reg::LR);
            let a: Vec<u32> = (0..4).map(|i| self.arg(i)).collect();
            let from = self.describe(lr);
            self.log.push(format!(
                "{kind}::{what}({:#x}, {:#x}, {:#x}, {:#x}) <- {from}",
                a[0], a[1], a[2], a[3]
            ));
        }

        const JNI_OK: u32 = 0;
        match (kind, what) {
            ("JNIEnv", "GetVersion") => self.ret(0x0001_0006), // JNI 1.6
            // (env, JavaVM** out) — the one JavaVM there is.
            ("JNIEnv", "GetJavaVM") => {
                let out = self.arg(1);
                self.mem.write_u32(out, VM_ADDR);
                self.ret(JNI_OK);
            }
            // (vm, JNIEnv** out, version|args) — this thread is always
            // "attached"; there is only one thread and one env.
            ("JavaVM", "GetEnv") | ("JavaVM", "AttachCurrentThread") => {
                let out = self.arg(1);
                self.mem.write_u32(out, ENV_ADDR);
                self.ret(JNI_OK);
            }
            ("JavaVM", "DetachCurrentThread") => self.ret(JNI_OK),

            // ---- the lookup registry --------------------------------------
            ("JNIEnv", "FindClass") => {
                let namep = self.arg(1);
                let name = self.mem.cstr(namep);
                let h = self.jni.intern(name);
                self.ret(h);
            }
            ("JNIEnv", "GetMethodID") | ("JNIEnv", "GetStaticMethodID") => {
                // (env, jclass, name, sig) -> a handle that *is* its own
                // description, so the first unimplemented Call* dies saying
                // which CletEntry callback the guest wanted.
                let cls = self.arg(1);
                let namep = self.arg(2);
                let sigp = self.arg(3);
                let owner = self.jni.describe(cls);
                let name = self.mem.cstr(namep);
                let sig = self.mem.cstr(sigp);
                let h = self.jni.intern(format!("{owner}.{name}{sig}"));
                self.ret(h);
            }
            ("JNIEnv", "GetFieldID") | ("JNIEnv", "GetStaticFieldID") => {
                let cls = self.arg(1);
                let namep = self.arg(2);
                let owner = self.jni.describe(cls);
                let name = self.mem.cstr(namep);
                let h = self.jni.intern(format!("{owner}.{name}"));
                self.ret(h);
            }
            // (env, obj, fieldID) -> the object's stored int (FileDescriptor).
            ("JNIEnv", "GetIntField") => {
                let obj = self.arg(1);
                let v = self.jni.int_fields.get(&obj).copied().unwrap_or(0);
                self.ret(v as u32);
            }
            ("JNIEnv", "SetIntField") => {
                let obj = self.arg(1);
                let v = self.arg(3) as i32;
                self.jni.int_fields.insert(obj, v);
                self.ret(0);
            }
            // References are identity: there is no GC on the other side.
            ("JNIEnv", "NewGlobalRef") | ("JNIEnv", "NewLocalRef") => {
                let r = self.arg(1);
                self.ret(r);
            }
            ("JNIEnv", "DeleteGlobalRef") | ("JNIEnv", "DeleteLocalRef") => self.ret(0),
            // No Java side exists, so no exception can be pending.
            ("JNIEnv", "ExceptionCheck") => self.ret(0),      // JNI_FALSE
            ("JNIEnv", "ExceptionOccurred") => self.ret(0),   // null jthrowable
            ("JNIEnv", "ExceptionClear") => self.ret(0),

            // ---- strings & arrays -----------------------------------------
            // (env, const char* utf) -> jstring
            ("JNIEnv", "NewStringUTF") => {
                let p = self.arg(1);
                let bytes = if p == 0 { Vec::new() } else { self.mem.cstr_bytes(p) };
                let h = self.new_string_utf(&bytes);
                self.ret(h);
            }
            // The handle IS the char buffer; GetStringUTFChars(env, str, isCopy?)
            ("JNIEnv", "GetStringUTFChars") => {
                let s = self.arg(1);
                let iscopy = self.arg(2);
                if iscopy != 0 {
                    self.mem.write_u32(iscopy, 0); // isCopy = JNI_FALSE
                }
                self.ret(s);
            }
            ("JNIEnv", "ReleaseStringUTFChars") => self.ret(0),
            ("JNIEnv", "GetStringUTFLength") => {
                let s = self.arg(1);
                let n = self.jni.lengths.get(&s).copied().unwrap_or(0);
                self.ret(n);
            }
            // (env, jsize len) -> jbyteArray
            ("JNIEnv", "NewByteArray") => {
                let len = self.arg(1);
                let h = self.new_byte_array(len);
                self.ret(h);
            }
            ("JNIEnv", "GetArrayLength") => {
                let a = self.arg(1);
                let n = self.jni.lengths.get(&a).copied().unwrap_or(0);
                self.ret(n);
            }
            // (env, array, isCopy?) -> pointer. The handle is the buffer.
            ("JNIEnv", "GetByteArrayElements") => {
                let a = self.arg(1);
                let iscopy = self.arg(2);
                if iscopy != 0 {
                    self.mem.write_u32(iscopy, 0);
                }
                self.ret(a);
            }
            // (env, array, ptr, mode) — buffer is shared in place; nothing to do.
            ("JNIEnv", "ReleaseByteArrayElements") => self.ret(0),
            // (env, array, start, len, buf) — copy out of the array.
            ("JNIEnv", "GetByteArrayRegion") => {
                let (a, start, len, buf) = (self.arg(1), self.arg(2), self.arg(3), self.arg(4));
                if let Some(src) = self.mem.slice(a + start, len).map(<[u8]>::to_vec) {
                    self.mem.write_bytes(buf, &src);
                }
                self.ret(0);
            }
            // (env, array, start, len, buf) — copy into the array.
            ("JNIEnv", "SetByteArrayRegion") => {
                let (a, start, len, buf) = (self.arg(1), self.arg(2), self.arg(3), self.arg(4));
                if let Some(src) = self.mem.slice(buf, len).map(<[u8]>::to_vec) {
                    self.mem.write_bytes(a + start, &src);
                }
                self.ret(0);
            }

            // ---- CletEntry callbacks: CallStatic*MethodV ------------------
            // The guest's inline varargs wrapper spills to a va_list and calls
            // the `V` slot (e.g. CallStaticIntMethodV = 130). r2 is the
            // jmethodID (its handle names the target), r3 is the va_list.
            ("JNIEnv", w) if w.starts_with("CallStatic") && w.ends_with("MethodV") => {
                let mid = self.arg(2);
                let full = self.jni.describe(mid);
                let method = full.strip_prefix("co/kr/eamobile/CletEntry.").unwrap_or(&full).to_string();
                let mut va = crate::fmt::VaSource::List { ptr: self.arg(3) };
                match self.clet_call(&method, &mut va)? {
                    crate::cletentry::CletRet::Word(v) => self.ret(v),
                    crate::cletentry::CletRet::Object(v) => self.ret(v),
                    crate::cletentry::CletRet::Long(v) => self.ret_pair(v),
                }
            }

            _ => {
                let lr = self.cpu.reg_get(Mode::User, reg::LR);
                let from = self.describe(lr);
                // A Call* through the registry can name its target — that is
                // the whole point of interning the lookups.
                let target = if what.starts_with("Call") {
                    let mid = self.arg(2);
                    format!(" on {}", self.jni.describe(mid))
                } else {
                    String::new()
                };
                return Err(format!(
                    "unimplemented {kind} slot {slot} ({what}){target}, called from {from}"
                ));
            }
        }
        Ok(())
    }
}

/// The JNI 1.6 `JNINativeInterface` layout, by index. Diagnostic only — an
/// unimplemented slot dies with this name attached, which turns "slot 167"
/// into "NewStringUTF" in the report.
pub fn env_slot_name(i: usize) -> &'static str {
    SLOT_NAMES.get(i).copied().unwrap_or("out-of-table")
}

pub fn vm_slot_name(i: usize) -> &'static str {
    const VM_NAMES: [&str; 8] = [
        "reserved0",
        "reserved1",
        "reserved2",
        "DestroyJavaVM",
        "AttachCurrentThread",
        "DetachCurrentThread",
        "GetEnv",
        "AttachCurrentThreadAsDaemon",
    ];
    VM_NAMES.get(i).copied().unwrap_or("out-of-table")
}

#[rustfmt::skip]
const SLOT_NAMES: [&str; ENV_SLOTS] = [
    "reserved0", "reserved1", "reserved2", "reserved3",
    "GetVersion",
    "DefineClass", "FindClass",
    "FromReflectedMethod", "FromReflectedField", "ToReflectedMethod",
    "GetSuperclass", "IsAssignableFrom", "ToReflectedField",
    "Throw", "ThrowNew",
    "ExceptionOccurred", "ExceptionDescribe", "ExceptionClear", "FatalError",
    "PushLocalFrame", "PopLocalFrame",
    "NewGlobalRef", "DeleteGlobalRef", "DeleteLocalRef",
    "IsSameObject", "NewLocalRef", "EnsureLocalCapacity",
    "AllocObject", "NewObject", "NewObjectV", "NewObjectA",
    "GetObjectClass", "IsInstanceOf", "GetMethodID",
    "CallObjectMethod", "CallObjectMethodV", "CallObjectMethodA",
    "CallBooleanMethod", "CallBooleanMethodV", "CallBooleanMethodA",
    "CallByteMethod", "CallByteMethodV", "CallByteMethodA",
    "CallCharMethod", "CallCharMethodV", "CallCharMethodA",
    "CallShortMethod", "CallShortMethodV", "CallShortMethodA",
    "CallIntMethod", "CallIntMethodV", "CallIntMethodA",
    "CallLongMethod", "CallLongMethodV", "CallLongMethodA",
    "CallFloatMethod", "CallFloatMethodV", "CallFloatMethodA",
    "CallDoubleMethod", "CallDoubleMethodV", "CallDoubleMethodA",
    "CallVoidMethod", "CallVoidMethodV", "CallVoidMethodA",
    "CallNonvirtualObjectMethod", "CallNonvirtualObjectMethodV", "CallNonvirtualObjectMethodA",
    "CallNonvirtualBooleanMethod", "CallNonvirtualBooleanMethodV", "CallNonvirtualBooleanMethodA",
    "CallNonvirtualByteMethod", "CallNonvirtualByteMethodV", "CallNonvirtualByteMethodA",
    "CallNonvirtualCharMethod", "CallNonvirtualCharMethodV", "CallNonvirtualCharMethodA",
    "CallNonvirtualShortMethod", "CallNonvirtualShortMethodV", "CallNonvirtualShortMethodA",
    "CallNonvirtualIntMethod", "CallNonvirtualIntMethodV", "CallNonvirtualIntMethodA",
    "CallNonvirtualLongMethod", "CallNonvirtualLongMethodV", "CallNonvirtualLongMethodA",
    "CallNonvirtualFloatMethod", "CallNonvirtualFloatMethodV", "CallNonvirtualFloatMethodA",
    "CallNonvirtualDoubleMethod", "CallNonvirtualDoubleMethodV", "CallNonvirtualDoubleMethodA",
    "CallNonvirtualVoidMethod", "CallNonvirtualVoidMethodV", "CallNonvirtualVoidMethodA",
    "GetFieldID",
    "GetObjectField", "GetBooleanField", "GetByteField", "GetCharField",
    "GetShortField", "GetIntField", "GetLongField", "GetFloatField", "GetDoubleField",
    "SetObjectField", "SetBooleanField", "SetByteField", "SetCharField",
    "SetShortField", "SetIntField", "SetLongField", "SetFloatField", "SetDoubleField",
    "GetStaticMethodID",
    "CallStaticObjectMethod", "CallStaticObjectMethodV", "CallStaticObjectMethodA",
    "CallStaticBooleanMethod", "CallStaticBooleanMethodV", "CallStaticBooleanMethodA",
    "CallStaticByteMethod", "CallStaticByteMethodV", "CallStaticByteMethodA",
    "CallStaticCharMethod", "CallStaticCharMethodV", "CallStaticCharMethodA",
    "CallStaticShortMethod", "CallStaticShortMethodV", "CallStaticShortMethodA",
    "CallStaticIntMethod", "CallStaticIntMethodV", "CallStaticIntMethodA",
    "CallStaticLongMethod", "CallStaticLongMethodV", "CallStaticLongMethodA",
    "CallStaticFloatMethod", "CallStaticFloatMethodV", "CallStaticFloatMethodA",
    "CallStaticDoubleMethod", "CallStaticDoubleMethodV", "CallStaticDoubleMethodA",
    "CallStaticVoidMethod", "CallStaticVoidMethodV", "CallStaticVoidMethodA",
    "GetStaticFieldID",
    "GetStaticObjectField", "GetStaticBooleanField", "GetStaticByteField", "GetStaticCharField",
    "GetStaticShortField", "GetStaticIntField", "GetStaticLongField", "GetStaticFloatField",
    "GetStaticDoubleField",
    "SetStaticObjectField", "SetStaticBooleanField", "SetStaticByteField", "SetStaticCharField",
    "SetStaticShortField", "SetStaticIntField", "SetStaticLongField", "SetStaticFloatField",
    "SetStaticDoubleField",
    "NewString", "GetStringLength", "GetStringChars", "ReleaseStringChars",
    "NewStringUTF", "GetStringUTFLength", "GetStringUTFChars", "ReleaseStringUTFChars",
    "GetArrayLength",
    "NewObjectArray", "GetObjectArrayElement", "SetObjectArrayElement",
    "NewBooleanArray", "NewByteArray", "NewCharArray", "NewShortArray",
    "NewIntArray", "NewLongArray", "NewFloatArray", "NewDoubleArray",
    "GetBooleanArrayElements", "GetByteArrayElements", "GetCharArrayElements",
    "GetShortArrayElements", "GetIntArrayElements", "GetLongArrayElements",
    "GetFloatArrayElements", "GetDoubleArrayElements",
    "ReleaseBooleanArrayElements", "ReleaseByteArrayElements", "ReleaseCharArrayElements",
    "ReleaseShortArrayElements", "ReleaseIntArrayElements", "ReleaseLongArrayElements",
    "ReleaseFloatArrayElements", "ReleaseDoubleArrayElements",
    "GetBooleanArrayRegion", "GetByteArrayRegion", "GetCharArrayRegion",
    "GetShortArrayRegion", "GetIntArrayRegion", "GetLongArrayRegion",
    "GetFloatArrayRegion", "GetDoubleArrayRegion",
    "SetBooleanArrayRegion", "SetByteArrayRegion", "SetCharArrayRegion",
    "SetShortArrayRegion", "SetIntArrayRegion", "SetLongArrayRegion",
    "SetFloatArrayRegion", "SetDoubleArrayRegion",
    "RegisterNatives", "UnregisterNatives",
    "MonitorEnter", "MonitorExit",
    "GetJavaVM",
    "GetStringRegion", "GetStringUTFRegion",
    "GetPrimitiveArrayCritical", "ReleasePrimitiveArrayCritical",
    "GetStringCritical", "ReleaseStringCritical",
    "NewWeakGlobalRef", "DeleteWeakGlobalRef",
    "ExceptionCheck",
    "NewDirectByteBuffer", "GetDirectBufferAddress", "GetDirectBufferCapacity",
    "GetObjectRefType",
];
