import java.util.Properties

plugins {
    id("com.android.application")
}

// Version lives in ../version.properties so build_android.ps1 -Release can bump
// the code without editing this file.
val versionProps = Properties().apply {
    val f = rootProject.file("version.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}
val appVersionCode = (versionProps.getProperty("versionCode") ?: "1").trim().toInt()
val appVersionName = (versionProps.getProperty("versionName") ?: "0.1.0").trim()

// ── release signing ─────────────────────────────────────────────────────────
// An unsigned APK cannot be installed. The keystore and its passwords are
// SECRETS and are read from outside the repo, first complete set wins:
//
//   1. environment:  HL5_KEYSTORE, HL5_KEYSTORE_PASSWORD, HL5_KEY_ALIAS,
//                    HL5_KEY_PASSWORD
//   2. gradle properties in %USERPROFILE%\.gradle\gradle.properties (NOT here):
//                    hl5.keystore, hl5.keystorePassword, hl5.keyAlias,
//                    hl5.keyPassword
//
// With neither, a release build still succeeds but is unsigned, and says so.
fun secret(env: String, prop: String): String? =
    (System.getenv(env) ?: providers.gradleProperty(prop).orNull)?.takeIf { it.isNotBlank() }

val ksPath = secret("HL5_KEYSTORE", "hl5.keystore")
val ksPass = secret("HL5_KEYSTORE_PASSWORD", "hl5.keystorePassword")
val ksAlias = secret("HL5_KEY_ALIAS", "hl5.keyAlias")
val ksKeyPass = secret("HL5_KEY_PASSWORD", "hl5.keyPassword")
val ksFile = ksPath?.let { file(it) }
val canSign = ksFile != null && ksFile.exists() && ksPass != null && ksAlias != null && ksKeyPass != null

android {
    namespace = "kr.heroeslore.five"
    compileSdk = 35
    ndkVersion = "25.1.8937393"

    defaultConfig {
        // Deliberately NOT co.kr.eamobile.HeroesLore5: a different
        // applicationId means this installs alongside the original 32-bit APK
        // rather than replacing it, and keeps its own /data/data directory —
        // which matters on the S9, where the original still runs and is the
        // answer key every frame is compared against.
        applicationId = "kr.heroeslore.five"
        // The original declares no targetSdkVersion at all, so Android treats
        // it as its minSdk of 5 — and Android 14+ refuses to install anything
        // that old. That is the *second* reason the original will not go on a
        // modern phone, after the missing 32-bit ABI (ANALYSIS.md §9).
        // 26 is where AAudio and the modern window-insets APIs start, far
        // below any device that would need this build.
        minSdk = 26
        targetSdk = 35
        versionCode = appVersionCode
        versionName = appVersionName
        ndk {
            // The whole point: one ABI, and it is the 64-bit one. The original
            // shipped lib/armeabi (ARMv5TE) and nothing else, which is exactly
            // why `adb install` answers INSTALL_FAILED_NO_MATCHING_ABIS on a
            // device without AArch32 support.
            abiFilters += "arm64-v8a"
        }
    }

    if (canSign) {
        signingConfigs {
            create("release") {
                storeFile = ksFile
                storePassword = ksPass
                keyAlias = ksAlias
                keyPassword = ksKeyPass
                enableV1Signing = true
                enableV2Signing = true
            }
        }
    }

    buildTypes {
        debug {
            // A debug build installs ALONGSIDE the release one instead of
            // demanding an uninstall. The release APK on the bench S25 is
            // signed with the `hero5` alias and carries the save; a
            // debug-signed APK cannot update it in place, and `adb uninstall`
            // is the only way out - which deletes /data/data, i.e. the save.
            // A separate applicationId sidesteps that trade entirely: the
            // measurement build gets its own data dir, and because it IS
            // debuggable, adb can write <files>/hl5_present.txt and
            // hl5_speed.txt - so the egl/soft and speed A/Bs need no rebuild
            // and no build switch. The Java package (`namespace`) is
            // unchanged, so no JNI name moves.
            applicationIdSuffix = ".bench"
            versionNameSuffix = "-bench"
        }
        release {
            isMinifyEnabled = false
            if (canSign) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    androidResources {
        // The guest binary is read once at startup and the archive is read
        // repeatedly by offset; leaving both uncompressed lets the
        // AssetManager hand them over without inflating 16 MB first.
        // data.vfs.mp3 is already on AAPT's default no-compress list by
        // extension — this makes it explicit and adds the guest image.
        noCompress += listOf("so", "mp3")
    }

    packaging {
        jniLibs {
            // cargo-ndk writes the already-stripped cdylib into
            // src/main/jniLibs; leave it as AGP found it.
            keepDebugSymbols += "**/*.so"
        }
    }
}

// games-activity 4.4.0 drags in the pre-1.8 split kotlin-stdlib-jdk7/jdk8
// (1.6.21) artifacts, whose classes were folded into the main kotlin-stdlib in
// 1.8+. That collides with appcompat 1.7.0's kotlin-stdlib 1.8.22 (duplicate
// class). Forcing the split artifacts to 1.8.22 (now empty stubs) resolves it.
configurations.all {
    resolutionStrategy {
        force(
            "org.jetbrains.kotlin:kotlin-stdlib-jdk7:1.8.22",
            "org.jetbrains.kotlin:kotlin-stdlib-jdk8:1.8.22",
        )
    }
}

if (gradle.startParameter.taskNames.any { it.contains("Release", ignoreCase = true) }) {
    if (canSign) {
        logger.lifecycle("==> release signing: ${ksFile!!.absolutePath} (alias $ksAlias), versionCode $appVersionCode")
    } else {
        logger.warn(
            "==> WARNING: release build will be UNSIGNED and CANNOT BE INSTALLED.\n" +
                "    Set HL5_KEYSTORE / HL5_KEYSTORE_PASSWORD / HL5_KEY_ALIAS / HL5_KEY_PASSWORD,\n" +
                "    or hl5.keystore / hl5.keystorePassword / hl5.keyAlias / hl5.keyPassword in\n" +
                "    %USERPROFILE%\\.gradle\\gradle.properties. See build_android.ps1 -NewKeystore."
        )
    }
}

dependencies {
    // Provides com.google.androidgamesdk.GameActivity, whose native methods
    // are implemented inside libmain.so (android-activity 0.6 bundles the
    // matching GameActivity 4.4.0 C++).
    implementation("androidx.games:games-activity:4.4.0")
    // GameActivity extends androidx.appcompat.app.AppCompatActivity.
    implementation("androidx.appcompat:appcompat:1.7.0")
}
