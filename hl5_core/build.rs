fn main() {
    println!("cargo:rerun-if-env-changed=HL5_INJECT_JIN_SLOT");
    // The content line, baked at build time from android_app/flavor.properties
    // by build_android.ps1. See BRANCHES.md.
    println!("cargo:rerun-if-env-changed=HL5_FLAVOR");
}
