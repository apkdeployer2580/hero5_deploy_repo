//! Put the guest thread on the fastest cores the device actually has.
//!
//! # Why this exists
//!
//! The interpreter is **one thread**, and on every CPU-bound scene that
//! thread *is* the frame budget (ANALYSIS.md §33.0: the gameplay scene spends
//! 18.9 ms of a 36 ms repaint inside the guest). So the largest single lever
//! on this device is not which code the thread runs — it is which core runs
//! it.
//!
//! Measured on the bench S25 (SM-S938N, Snapdragon 8 Elite) by sampling every
//! app thread's `task/*/stat` field 39 once a second across four 70-second
//! runs: **not one sample landed on cpu 6 or 7.** The scheduler kept the whole
//! process on the performance cluster while the two prime cores idled at their
//! 1,017 MHz floor. That is not a permissions wall — `top-app`'s cpuset is
//! `0-7` — it is EAS declining to up-migrate a thread whose duty cycle sits
//! near 60%: the perf cluster is "big enough", so the placement is
//! energy-optimal and latency-blind.
//!
//! What being right about energy costs here:
//!
//! | | performance cluster | prime cluster |
//! |---|---|---|
//! | `cpuinfo_max_freq` | 3,532 MHz | **4,473 MHz** |
//! | same shell loop, run twice | 350 / 352 ms | **197 / 200 ms** |
//!
//! **1.77×** — more than the clock ratio alone (1.27×), because these are
//! wider cores and not merely faster-clocked ones. For scale: the three
//! remaining blitter ports together hold 71% of a guest tick's profile share
//! (§32.6 tail), and each is a day of disassembly plus a 1,241-frame A/B.
//! This is one `sched_setaffinity` call.
//!
//! # Why the app has to do it itself
//!
//! `taskset` from an adb shell — even through `run-as`, i.e. as the app's own
//! uid — answers `Permission denied`: SELinux gates `setsched` on the *target*
//! process's domain and the shell domain is not it. A thread setting **its
//! own** affinity is the case that is always allowed, so the call has to
//! happen on the guest thread, inside the app.
//!
//! # Why it is re-applied on every resume
//!
//! Affinity is intersected with the thread's cpuset, and Android moves an app
//! between cpusets as it goes foreground and background (`top-app` is `0-7`,
//! `foreground` is `0-5`, `background` is `0-1,4-5`). A backgrounded app whose
//! mask no longer intersects its cpuset has that mask reset by the kernel, so
//! the pin does not survive a home-button round trip on its own. `resumed`
//! re-applies it; the call is idempotent and the caller logs only changes.
//!
//! # Why nothing is hardcoded
//!
//! `6-7` is this phone's answer, not the answer. The cluster is derived at
//! runtime from `cpuinfo_max_freq`, so a device whose cores are all one speed
//! gets no pin at all rather than an arbitrary half of them, and a device that
//! numbers its prime cores first gets those.

/// Confine the calling thread to the highest-`cpuinfo_max_freq` cluster.
///
/// `Ok(note)` describes what is now in force — the caller logs it when it
/// changes, so a resume that re-applies the same mask stays quiet. `Err(why)`
/// is a reason the pin did not happen, including the deliberate ones (a device
/// whose cores are all the same speed).
#[cfg(target_os = "android")]
pub fn pin_to_fastest() -> Result<String, String> {
    let clusters = clusters()?;
    let top = clusters
        .iter()
        .map(|c| c.max_khz)
        .max()
        .ok_or_else(|| "no cpufreq policies".to_string())?;
    let fast: Vec<u32> = clusters
        .iter()
        .filter(|c| c.max_khz == top)
        .flat_map(|c| c.cpus.iter().copied())
        .collect();
    let all: usize = clusters.iter().map(|c| c.cpus.len()).sum();
    if fast.len() == all {
        // One speed for every core: the mask would be the mask the thread
        // already has. Say so rather than report a pin that did nothing.
        return Err(format!(
            "every core caps at {} MHz; no cluster to prefer",
            top / 1000
        ));
    }

    let mut mask = CpuSet::new();
    for &c in &fast {
        mask.set(c);
    }
    // pid 0 is "the calling thread" — the one case SELinux does not gate.
    let rc = unsafe { sched_setaffinity(0, CPU_SET_BYTES, &mask) };
    if rc != 0 {
        return Err(format!("sched_setaffinity({}) refused", list(&fast)));
    }

    // Report what the kernel actually installed rather than what was asked
    // for: the cpuset intersection happens inside the call and can quietly
    // narrow it.
    let mut got = CpuSet::new();
    let now = if unsafe { sched_getaffinity(0, CPU_SET_BYTES, &mut got) } == 0 {
        list(&got.cpus())
    } else {
        list(&fast)
    };
    let rest = clusters
        .iter()
        .filter(|c| c.max_khz != top)
        .map(|c| c.max_khz)
        .max();
    Ok(match rest {
        Some(r) => format!(
            "guest thread on cpus {now} ({} MHz; the rest cap at {} MHz)",
            top / 1000,
            r / 1000
        ),
        None => format!("guest thread on cpus {now} ({} MHz)", top / 1000),
    })
}

#[cfg(not(target_os = "android"))]
pub fn pin_to_fastest() -> Result<String, String> {
    Err("only implemented for android".to_string())
}

#[cfg(target_os = "android")]
struct Cluster {
    cpus: Vec<u32>,
    max_khz: u32,
}

/// The CPU clusters, taken from cpufreq's own grouping.
///
/// `related_cpus` is the set of CPUs sharing one clock domain, which is
/// exactly the grouping wanted here — deriving it from `cpu_capacity` or from
/// core numbering would guess at something sysfs states.
#[cfg(target_os = "android")]
fn clusters() -> Result<Vec<Cluster>, String> {
    let root = std::path::Path::new("/sys/devices/system/cpu/cpufreq");
    let dir = std::fs::read_dir(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let mut out = Vec::new();
    for entry in dir.flatten() {
        let p = entry.path();
        if !p
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("policy"))
        {
            continue;
        }
        let Ok(freq) = std::fs::read_to_string(p.join("cpuinfo_max_freq")) else {
            continue;
        };
        let Ok(max_khz) = freq.trim().parse::<u32>() else {
            continue;
        };
        let cpus: Vec<u32> = std::fs::read_to_string(p.join("related_cpus"))
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if !cpus.is_empty() {
            out.push(Cluster { cpus, max_khz });
        }
    }
    if out.is_empty() {
        return Err("cpufreq exposes no policies".to_string());
    }
    Ok(out)
}

/// `[0, 1, 4, 5, 6]` becomes `"0-1,4-6"`. Ranges because a mask printed as
/// eight separate numbers is unreadable in a logcat line that already carries
/// a dozen figures.
#[cfg(target_os = "android")]
fn list(cpus: &[u32]) -> String {
    let mut v = cpus.to_vec();
    v.sort_unstable();
    v.dedup();
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let start = v[i];
        let mut end = start;
        while i + 1 < v.len() && v[i + 1] == end + 1 {
            i += 1;
            end = v[i];
        }
        parts.push(if start == end {
            start.to_string()
        } else {
            format!("{start}-{end}")
        });
        i += 1;
    }
    parts.join(",")
}

// --- the two syscalls -------------------------------------------------------
//
// Declared by hand against bionic rather than pulling in `libc`, for the same
// reason `egl` declares its own: two symbols do not earn a dependency, and
// `cpu_set_t` has exactly one property that matters here, its size.

/// bionic's `cpu_set_t` is 1,024 bits. The kernel accepts a shorter mask, but
/// handing it the size the platform's own headers use removes the question.
#[cfg(target_os = "android")]
const CPU_SET_BYTES: usize = 128;

#[cfg(target_os = "android")]
#[repr(C)]
struct CpuSet([u64; CPU_SET_BYTES / 8]);

#[cfg(target_os = "android")]
impl CpuSet {
    fn new() -> Self {
        CpuSet([0; CPU_SET_BYTES / 8])
    }

    fn set(&mut self, cpu: u32) {
        let (w, b) = (cpu as usize / 64, cpu as usize % 64);
        if w < self.0.len() {
            self.0[w] |= 1u64 << b;
        }
    }

    fn cpus(&self) -> Vec<u32> {
        let mut v = Vec::new();
        for (w, word) in self.0.iter().enumerate() {
            for b in 0..64 {
                if word & (1u64 << b) != 0 {
                    v.push((w * 64 + b) as u32);
                }
            }
        }
        v
    }
}

#[cfg(target_os = "android")]
extern "C" {
    fn sched_setaffinity(pid: i32, cpusetsize: usize, mask: *const CpuSet) -> i32;
    fn sched_getaffinity(pid: i32, cpusetsize: usize, mask: *mut CpuSet) -> i32;
}

#[cfg(all(test, target_os = "android"))]
mod tests {
    use super::list;

    #[test]
    fn ranges_collapse() {
        assert_eq!(list(&[6, 7]), "6-7");
        assert_eq!(list(&[0, 1, 4, 5, 6]), "0-1,4-6");
        assert_eq!(list(&[3]), "3");
        assert_eq!(list(&[]), "");
    }
}
