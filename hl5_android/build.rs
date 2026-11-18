//! Tell cargo that the baked-in switches are inputs.
//!
//! `option_env!` reads a variable at compile time, but cargo does not track
//! it on its own: changing it and rebuilding would quietly reuse the old
//! object and ship a build carrying the previous value. That failure is
//! invisible — the APK installs, launches, and behaves like last week's
//! flags — so every `option_env!` this crate reads is declared here. HL5_BOOT
//! had been missing from this list since the switch was added; a `-Boot`
//! change without a source change was a no-op build.

fn main() {
    println!("cargo:rerun-if-env-changed=HL5_SERVER");
    println!("cargo:rerun-if-env-changed=HL5_BOOT");
    println!("cargo:rerun-if-env-changed=HL5_SPEED");
    println!("cargo:rerun-if-env-changed=HL5_PRESENT");
    println!("cargo:rerun-if-env-changed=HL5_AFFINITY");
    println!("cargo:rerun-if-env-changed=HL5_POLL");
}
