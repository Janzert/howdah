//! What the operating system can tell us about, and do to, an engine's
//! processes: its resident memory, the machine's memory, a memory cap the
//! kernel enforces, and killing the engine with everything it started.
//!
//! - Linux: resident memory from `/proc/<pid>/statm`; the cap is a systemd
//!   user scope (`systemd-run --user --scope -p MemoryMax=… -p
//!   MemorySwapMax=0`, which execs the engine, so its pid stays ours) when
//!   `systemd-run` works; the engine's process group is killed with
//!   `killpg`.
//! - Windows: the working set; every engine runs in a job object that kills
//!   it and its children when the job is closed or terminated (and so when
//!   we exit), with a per-process commit limit as the cap.
//! - macOS: resident memory from `proc_pidinfo`; no cap; the process group
//!   is killed as on Linux.

use std::path::{Path, PathBuf};

/// Bytes the process `pid` has resident (its working set on Windows), if
/// it can be read. Only that process, not its children.
pub fn resident_memory(pid: u32) -> Option<u64> {
    imp::resident_memory(pid)
}

/// The machine's physical memory in bytes, if it can be read.
pub fn physical_memory() -> Option<u64> {
    imp::physical_memory()
}

/// How a memory cap is enforced on this machine, if one can be: the
/// kernel then kills an engine that passes it, instead of the machine
/// running out of memory.
pub fn memory_cap_method() -> Option<&'static str> {
    imp::memory_cap_method()
}

/// The program and arguments that run `program` under a memory cap of
/// `limit` bytes, where that's done by wrapping the command.
pub(crate) fn capped_command(program: &Path, args: &[String], limit: u64) -> Option<(PathBuf, Vec<String>)> {
    imp::capped_command(program, args, limit)
}

/// Whether this process has a console window. Without one (the app, a GUI
/// program), Windows gives every console program it starts a window of
/// its own unless it's started with `CREATE_NO_WINDOW`.
#[cfg(windows)]
pub(crate) fn has_console() -> bool {
    // SAFETY: GetConsoleWindow takes nothing and only returns a handle.
    !unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() }.is_null()
}

/// Kills process group `pgid`. Only call it while the group's leader
/// hasn't been reaped, or just after: a group id is never handed to a new
/// process while any member is alive, and pids are reused only after
/// wrapping around.
#[cfg(unix)]
pub(crate) fn kill_group(pgid: u32) {
    // SAFETY: killpg has no memory-safety preconditions.
    unsafe {
        libc::killpg(pgid as libc::pid_t, libc::SIGKILL);
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::OnceLock;

    pub fn resident_memory(pid: u32) -> Option<u64> {
        let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
        let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        // SAFETY: sysconf has no memory-safety preconditions.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        Some(pages * u64::try_from(page).ok()?)
    }

    pub fn physical_memory() -> Option<u64> {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        let line = meminfo.lines().find(|l| l.starts_with("MemTotal:"))?;
        let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb * 1024)
    }

    /// Whether `systemd-run --user --scope` works here (a systemd user
    /// session, with its memory controller). Tried once, with a scope
    /// running `true`.
    fn systemd_scopes() -> bool {
        static WORKS: OnceLock<bool> = OnceLock::new();
        *WORKS.get_or_init(|| {
            Command::new("systemd-run")
                .args(["--user", "--scope", "--quiet", "--collect", "-p", "MemoryMax=64M", "-p"])
                .args(["MemorySwapMax=0", "--", "true"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        })
    }

    pub fn memory_cap_method() -> Option<&'static str> {
        systemd_scopes().then_some("a systemd user scope")
    }

    pub fn capped_command(program: &Path, args: &[String], limit: u64) -> Option<(PathBuf, Vec<String>)> {
        if !systemd_scopes() {
            return None;
        }
        let mut wrapped: Vec<String> =
            ["--user", "--scope", "--quiet", "--collect", "-p"].into_iter().map(String::from).collect();
        wrapped.push(format!("MemoryMax={limit}"));
        wrapped.extend(["-p".into(), "MemorySwapMax=0".into(), "--".into()]);
        wrapped.push(program.to_string_lossy().into_owned());
        wrapped.extend(args.iter().cloned());
        Some(("systemd-run".into(), wrapped))
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::{Path, PathBuf};

    pub fn resident_memory(pid: u32) -> Option<u64> {
        let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        // SAFETY: `info` is a proc_taskinfo of the size given.
        let got = unsafe {
            libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTASKINFO, 0, (&raw mut info).cast(), size)
        };
        (got == size).then_some(info.pti_resident_size)
    }

    pub fn physical_memory() -> Option<u64> {
        let mut bytes: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        // SAFETY: hw.memsize is a u64, and `len` says so.
        let ok = unsafe {
            libc::sysctlbyname(
                c"hw.memsize".as_ptr(),
                (&raw mut bytes).cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        (ok == 0).then_some(bytes)
    }

    pub fn memory_cap_method() -> Option<&'static str> {
        None
    }

    pub fn capped_command(_: &Path, _: &[String], _: u64) -> Option<(PathBuf, Vec<String>)> {
        None
    }
}

#[cfg(windows)]
mod imp {
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    pub fn resident_memory(pid: u32) -> Option<u64> {
        // SAFETY: the handle is checked and closed; the counters are the
        // size given.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            counters.cb = size;
            let ok = K32GetProcessMemoryInfo(handle, &mut counters, size);
            CloseHandle(handle);
            (ok != 0).then_some(counters.WorkingSetSize as u64)
        }
    }

    pub fn physical_memory() -> Option<u64> {
        // SAFETY: dwLength is set as the call requires.
        unsafe {
            let mut status: MEMORYSTATUSEX = std::mem::zeroed();
            status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            (GlobalMemoryStatusEx(&mut status) != 0).then_some(status.ullTotalPhys)
        }
    }

    pub fn memory_cap_method() -> Option<&'static str> {
        Some("a job object")
    }

    /// Windows caps through the engine's job object instead.
    pub fn capped_command(_: &Path, _: &[String], _: u64) -> Option<(PathBuf, Vec<String>)> {
        None
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod imp {
    use std::path::{Path, PathBuf};

    pub fn resident_memory(_: u32) -> Option<u64> {
        None
    }

    pub fn physical_memory() -> Option<u64> {
        None
    }

    pub fn memory_cap_method() -> Option<&'static str> {
        None
    }

    pub fn capped_command(_: &Path, _: &[String], _: u64) -> Option<(PathBuf, Vec<String>)> {
        None
    }
}

/// A job object holding one engine and whatever it starts. Closing it
/// (dropping this, or our process ending) kills them all.
#[cfg(windows)]
pub(crate) struct Job(windows_sys::Win32::Foundation::HANDLE);

// SAFETY: a job handle can be used from any thread.
#[cfg(windows)]
unsafe impl Send for Job {}
#[cfg(windows)]
unsafe impl Sync for Job {}

#[cfg(windows)]
impl Job {
    /// Makes a job that kills its processes when closed and, with a
    /// `memory_limit`, any process committing more than that many bytes,
    /// and puts `process` (a process handle) in it. None if that fails
    /// (the engine then runs without one).
    pub(crate) fn assign(process: std::os::windows::io::RawHandle, memory_limit: Option<u64>) -> Option<Job> {
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
        };
        // SAFETY: the handles are checked; the limit information is the
        // size given.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return None;
            }
            let job = Job(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if let Some(limit) = memory_limit {
                info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY;
                info.ProcessMemoryLimit = usize::try_from(limit).unwrap_or(usize::MAX);
            }
            let set = SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if set == 0 || AssignProcessToJobObject(job.0, process as _) == 0 {
                return None;
            }
            Some(job)
        }
    }

    /// Kills every process in the job.
    pub(crate) fn terminate(&self) {
        // SAFETY: the handle is ours until dropped.
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle is ours, closed once.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
