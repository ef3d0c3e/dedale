//! Running bots in a sandbox: bubblewrap isolates them, `prlimit`
//! caps their resources from inside, `taskset` gives each a core of its own.
//!
//! Linux only, and opt-in. A sandboxed bot sees the system read-only (`/usr`
//! and the libraries), its own directory and the interpreter that runs it
//! read-only, and a small private `/tmp`; it has no network, no environment
//! but a few variables, and no other process in sight.
//!
//! The caps are set inside the sandbox on purpose: in its own user
//! namespace a bot's processes are counted apart from the user's others.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use nix::sched::{CpuSet, sched_getaffinity, sched_setaffinity};
use nix::unistd::Pid;
use thiserror::Error;

/// Directories of the system every bot sees, read-only, when they exist.
const SYSTEM: [&str; 5] = ["/bin", "/sbin", "/lib", "/lib32", "/lib64"];
/// What the dynamic loader and the C library read in `/etc`.
const ETC: [&str; 6] = [
    "/etc/ld.so.cache",
    "/etc/ld.so.conf",
    "/etc/ld.so.conf.d",
    "/etc/alternatives",
    "/etc/localtime",
    "/etc/nsswitch.conf",
];
/// The whole environment of a sandboxed bot, besides its command's own.
const ENV: [(&str, &str); 3] = [
    ("PATH", "/usr/local/bin:/usr/bin:/bin"),
    ("HOME", "/tmp"),
    ("LANG", "C.UTF-8"),
];
const MIB: u64 = 1 << 20;

/// What a sandboxed bot may use, per bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Address space of each of its processes, in MiB.
    pub memory_mib: u64,
    /// Processes and threads, all together.
    pub processes: u64,
    /// Size of its `/tmp`, and of any file it writes, in MiB.
    pub tmp_mib: u64,
    /// Open files per process.
    pub open_files: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_mib: 2048,
            processes: 64,
            tmp_mib: 64,
            open_files: 256,
        }
    }
}

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error(
        "the sandbox cannot run here: {what}{}\n{HINT}",
        if detail.is_empty() { String::new() } else { format!(": {detail}") }
    )]
    Unusable { what: String, detail: String },
}

const HINT: &str = "It needs Linux with bubblewrap (bwrap), prlimit and taskset (util-linux). \
     On Ubuntu 24.04 and later, AppArmor stops unprivileged user namespaces: load bwrap's \
     profile or run `sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0`.";

/// How bots are sandboxed, checked to work on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sandbox {
    pub limits: Limits,
    bwrap: PathBuf,
    /// The interpreter Python bots run with, when it could be found: its
    /// real executable (past any shim) and its installation directories.
    python: Option<Interpreter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Interpreter {
    executable: PathBuf,
    prefixes: Vec<PathBuf>,
}

impl Sandbox {
    /// A sandbox with `bwrap` from `PATH`, Python bots run by `python`.
    ///
    /// # Errors
    ///
    /// When bubblewrap, `prlimit` or `taskset` does not work here.
    pub fn new(limits: Limits, python: &OsStr) -> Result<Self, SandboxError> {
        Self::with_bwrap(Path::new("bwrap"), limits, python)
    }

    /// A sandbox run by the given `bwrap`.
    ///
    /// # Errors
    ///
    /// When it, `prlimit` or `taskset` does not work here.
    pub fn with_bwrap(bwrap: &Path, limits: Limits, python: &OsStr) -> Result<Self, SandboxError> {
        let sandbox = Self {
            limits,
            bwrap: bwrap.to_owned(),
            python: Interpreter::find(python),
        };
        let cpu = allowed_cpus().first().copied().unwrap_or(0);
        let mut test = sandbox.command(
            Path::new("true"),
            &[],
            &[],
            &[],
            Path::new("/tmp"),
            Some(cpu),
        );
        check(&mut test, "a test command failed in the sandbox")?;
        Ok(sandbox)
    }

    /// The executable Python bots run with in the sandbox, when found.
    #[must_use]
    pub fn python(&self) -> Option<&Path> {
        self.python.as_ref().map(|p| p.executable.as_path())
    }

    /// `program args…` in the sandbox, with `env` set, seeing `visible` too
    /// (read-only), starting in `dir`, and pinned to `cpu` when given.
    #[must_use]
    pub fn command(
        &self,
        program: &Path,
        args: &[&OsStr],
        env: &[(&str, &str)],
        visible: &[&Path],
        dir: &Path,
        cpu: Option<usize>,
    ) -> Command {
        let mut command = match cpu {
            Some(cpu) => {
                let mut taskset = Command::new("taskset");
                taskset
                    .arg("--cpu-list")
                    .arg(cpu.to_string())
                    .arg(&self.bwrap);
                taskset
            }
            None => Command::new(&self.bwrap),
        };
        command.args(self.bwrap_args(env, visible, dir));
        command
            .arg("--")
            .args(self.prlimit_args())
            .arg(program)
            .args(args);
        command
    }

    fn bwrap_args(&self, env: &[(&str, &str)], visible: &[&Path], dir: &Path) -> Vec<OsString> {
        let mut args: Vec<OsString> = [
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--clearenv",
        ]
        .map(OsString::from)
        .into();
        for (name, value) in ENV.iter().chain(env) {
            args.extend(["--setenv", name, value].map(OsString::from));
        }
        args.extend(["--ro-bind", "/usr", "/usr"].map(OsString::from));
        for path in SYSTEM {
            match fs::read_link(path) {
                Ok(target) => {
                    args.extend([OsString::from("--symlink"), target.into(), path.into()]);
                }
                Err(_) if Path::new(path).is_dir() => {
                    args.extend(["--ro-bind", path, path].map(OsString::from));
                }
                Err(_) => {}
            }
        }
        for path in ETC {
            args.extend(["--ro-bind-try", path, path].map(OsString::from));
        }
        args.extend(["--proc", "/proc", "--dev", "/dev", "--size"].map(OsString::from));
        args.push((self.limits.tmp_mib * MIB).to_string().into());
        args.extend(["--tmpfs", "/tmp"].map(OsString::from));
        let prefixes = self.python.iter().flat_map(|p| &p.prefixes);
        for path in prefixes
            .map(PathBuf::as_path)
            .chain(visible.iter().copied())
        {
            args.extend([OsString::from("--ro-bind"), path.into(), path.into()]);
        }
        args.extend([OsString::from("--chdir"), dir.into()]);
        args
    }

    fn prlimit_args(&self) -> Vec<OsString> {
        let l = &self.limits;
        let file = l.tmp_mib * MIB;
        [
            "prlimit".to_owned(),
            format!("--as={}", l.memory_mib * MIB),
            format!("--nproc={}", l.processes),
            format!("--fsize={file}"),
            format!("--nofile={}", l.open_files),
            "--".to_owned(),
        ]
        .map(OsString::from)
        .into()
    }
}

impl Interpreter {
    /// What `python` really runs, asked of itself: shims such as pyenv's
    /// would not work in the sandbox, and an interpreter installed outside
    /// `/usr` must be made visible.
    fn find(python: &OsStr) -> Option<Self> {
        let script = "import sys; print(sys.executable); print(sys.base_prefix); print(sys.prefix)";
        let output = Command::new(python)
            .args(["-I", "-c", script])
            .stdin(Stdio::null())
            .output()
            .ok()?;
        let text = String::from_utf8(output.stdout).ok()?;
        let mut lines = text.lines().map(PathBuf::from);
        let executable = lines.next().filter(|p| p.is_absolute())?;
        let mut prefixes: Vec<PathBuf> = Vec::new();
        for prefix in lines.filter(|p| p.is_absolute() && !p.starts_with("/usr")) {
            if !prefixes.contains(&prefix) {
                prefixes.push(prefix);
            }
        }
        Some(Self {
            executable,
            prefixes,
        })
    }
}

fn check(command: &mut Command, what: &str) -> Result<(), SandboxError> {
    let unusable = |detail: String| SandboxError::Unusable {
        what: what.to_owned(),
        detail,
    };
    let output = command.stdin(Stdio::null()).output().map_err(|e| {
        unusable(format!(
            "cannot start {}: {e}",
            command.get_program().display()
        ))
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(unusable(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

/// The CPUs this process may run on, in order.
#[must_use]
pub fn allowed_cpus() -> Vec<usize> {
    let Ok(set) = sched_getaffinity(Pid::from_raw(0)) else {
        return Vec::new();
    };
    (0..CpuSet::count())
        .filter(|&cpu| set.is_set(cpu).unwrap_or(false))
        .collect()
}

/// A CPU for each of `bots` bots: on separate physical cores when there
/// are enough, from the last (the first often handles interrupts), and
/// shared in turn when there are more bots than CPUs.
#[must_use]
pub fn cores(bots: usize) -> Vec<usize> {
    let allowed = allowed_cpus();
    let order = spread(&allowed, |cpu| {
        let path = format!("/sys/devices/system/cpu/cpu{cpu}/topology/thread_siblings_list");
        fs::read_to_string(path)
            .map(|list| parse_cpu_list(&list))
            .unwrap_or_default()
    });
    if order.is_empty() {
        return Vec::new();
    }
    (0..bots).map(|bot| order[bot % order.len()]).collect()
}

/// `allowed` CPUs from the last, one per physical core first (as told by
/// `siblings`), then the others.
fn spread(allowed: &[usize], siblings: impl Fn(usize) -> Vec<usize>) -> Vec<usize> {
    let mut first: Vec<usize> = Vec::new();
    let mut then: Vec<usize> = Vec::new();
    for &cpu in allowed.iter().rev() {
        let shared = siblings(cpu).iter().any(|s| first.contains(s));
        if shared { &mut then } else { &mut first }.push(cpu);
    }
    first.extend(then);
    first
}

/// The CPUs of a kernel CPU list, such as `0-3,8`.
fn parse_cpu_list(list: &str) -> Vec<usize> {
    let mut cpus = Vec::new();
    for part in list.trim().split(',').filter(|p| !p.is_empty()) {
        let bounds = part.split_once('-').unwrap_or((part, part));
        if let (Ok(start), Ok(end)) = (bounds.0.parse::<usize>(), bounds.1.parse::<usize>()) {
            cpus.extend(start..=end);
        }
    }
    cpus
}

/// Keeps this process off the bots' CPUs when others remain, so Dedale
/// never takes their time. Threads started afterwards follow.
pub fn keep_off(cpus: &[usize]) {
    let mut set = CpuSet::new();
    let mut any = false;
    for cpu in allowed_cpus().into_iter().filter(|c| !cpus.contains(c)) {
        any |= set.set(cpu).is_ok();
    }
    if any {
        let _ = sched_setaffinity(Pid::from_raw(0), &set);
    }
}

#[cfg(test)]
mod tests {
    use super::{Limits, Sandbox, parse_cpu_list, spread};
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    #[test]
    fn cpu_lists() {
        assert_eq!(parse_cpu_list("0-3,8\n"), [0, 1, 2, 3, 8]);
        assert_eq!(parse_cpu_list("5"), [5]);
        assert_eq!(parse_cpu_list(""), Vec::<usize>::new());
        assert_eq!(parse_cpu_list("x,2"), [2]);
    }

    #[test]
    fn cores_are_spread_over_physical_cores() {
        // Four CPUs, two physical cores: 0 and 2 are siblings, 1 and 3.
        let siblings = |cpu: usize| vec![cpu % 2, cpu % 2 + 2];
        assert_eq!(spread(&[0, 1, 2, 3], siblings), [3, 2, 1, 0]);
        // Without topology, every CPU counts as its own core.
        assert_eq!(spread(&[4, 6], |_| Vec::new()), [6, 4]);
    }

    #[test]
    fn commands_hide_the_host() {
        let sandbox = Sandbox {
            limits: Limits::default(),
            bwrap: PathBuf::from("bwrap"),
            python: None,
        };
        let command = sandbox.command(
            Path::new("/usr/bin/python3"),
            &[OsStr::new("bot.py")],
            &[("PYTHONDONTWRITEBYTECODE", "1")],
            &[Path::new("/home/ada/bots/wall")],
            Path::new("/home/ada/bots/wall"),
            Some(3),
        );
        assert_eq!(command.get_program(), "taskset");
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy()).collect();
        let text = args.join(" ");
        assert!(
            text.starts_with("--cpu-list 3 bwrap --unshare-all"),
            "{text}"
        );
        for part in [
            "--clearenv",
            "--setenv HOME /tmp",
            "--setenv PYTHONDONTWRITEBYTECODE 1",
            "--ro-bind /usr /usr",
            "--size 67108864 --tmpfs /tmp",
            "--ro-bind /home/ada/bots/wall /home/ada/bots/wall",
            "--chdir /home/ada/bots/wall",
            "-- prlimit --as=2147483648 --nproc=64 --fsize=67108864 --nofile=256 -- /usr/bin/python3 bot.py",
        ] {
            assert!(text.contains(part), "{part} in {text}");
        }
        assert!(!text.contains("--bind "), "nothing is writable: {text}");
    }

    #[test]
    fn a_missing_bubblewrap_is_an_error() {
        let error = Sandbox::with_bwrap(
            Path::new("/nonexistent/bwrap"),
            Limits::default(),
            OsStr::new("python3"),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("cannot run here"), "{error}");
        assert!(
            error.contains("apparmor_restrict_unprivileged_userns"),
            "{error}"
        );
    }
}
