//! Pane ownership: Unix terminal session (new sessions deliberately detach),
//! Windows kill-on-close job. Never signal the application's own session.

#[cfg(unix)]
pub struct ProcessOwner {
    session_id: libc::pid_t,
}

#[cfg(unix)]
impl ProcessOwner {
    pub fn new(child: &dyn portable_pty::Child) -> anyhow::Result<Self> {
        let pid = child
            .process_id()
            .ok_or_else(|| anyhow::anyhow!("PTY child has no PID"))?
            as libc::pid_t;
        // portable-pty establishes a new session before exec; even a very short
        // command may already have exited, so retain its known session ID.
        anyhow::ensure!(
            pid > 1 && pid != unsafe { libc::getsid(0) },
            "unsafe PTY session"
        );
        Ok(Self { session_id: pid })
    }

    pub fn terminate(&self) -> anyhow::Result<()> {
        terminate_sessions(&[self.session_id])
    }

    pub fn terminate_all<'a>(owners: impl Iterator<Item = &'a Self>) -> anyhow::Result<()> {
        let mut ids: Vec<_> = owners.map(|owner| owner.session_id).collect();
        ids.sort_unstable();
        terminate_sessions(&ids)
    }
}

#[cfg(unix)]
fn terminate_sessions(ids: &[libc::pid_t]) -> anyhow::Result<()> {
    use std::time::{Duration, Instant};
    if ids.is_empty() {
        return Ok(());
    }
    let mut system = sysinfo::System::new();
    let deadline = Instant::now() + Duration::from_millis(750);
    let mut first = true;
    loop {
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::nothing(),
        );
        let mut live = false;
        // Stop the whole ownership set before resuming any member. Subsequent
        // scans catch children forked just before their parent was stopped.
        let signals: &[i32] = if first {
            &[libc::SIGSTOP, libc::SIGTERM, libc::SIGCONT]
        } else {
            &[libc::SIGKILL]
        };
        for &signal in signals {
            for (&pid, process) in system.processes() {
                let pid = pid.as_u32() as libc::pid_t;
                let sid = unsafe { libc::getsid(pid) };
                if sid <= 1
                    || ids.binary_search(&sid).is_err()
                    || process.status() == sysinfo::ProcessStatus::Zombie
                {
                    continue;
                }
                live = true;
                if unsafe { libc::kill(pid, signal) } != 0 {
                    let error = std::io::Error::last_os_error();
                    if error.raw_os_error() != Some(libc::ESRCH) {
                        return Err(error.into());
                    }
                }
            }
        }
        if !live {
            return Ok(());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "owned PTY sessions did not terminate within 750ms"
        );
        std::thread::sleep(Duration::from_millis(if first { 100 } else { 20 }));
        first = false;
    }
}

#[cfg(windows)]
pub struct ProcessOwner {
    job: windows_sys::Win32::Foundation::HANDLE,
}
// The job handle has no thread-affine operations; its lifetime is owned here.
#[cfg(windows)]
unsafe impl Send for ProcessOwner {}
#[cfg(windows)]
unsafe impl Sync for ProcessOwner {}

#[cfg(windows)]
impl ProcessOwner {
    pub fn new(child: &dyn portable_pty::Child) -> anyhow::Result<Self> {
        use windows_sys::Win32::System::JobObjects::*;
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        anyhow::ensure!(
            !job.is_null(),
            "create pane job: {}",
            std::io::Error::last_os_error()
        );
        let owner = Self { job };
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let process = child
            .as_raw_handle()
            .ok_or_else(|| anyhow::anyhow!("PTY child has no process handle"))?;
        if unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
            || unsafe { AssignProcessToJobObject(job, process) } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(owner)
    }

    pub fn terminate(&self) -> anyhow::Result<()> {
        terminate_jobs(&[self.job])
    }

    pub fn terminate_all<'a>(owners: impl Iterator<Item = &'a Self>) -> anyhow::Result<()> {
        let jobs: Vec<_> = owners.map(|owner| owner.job).collect();
        terminate_jobs(&jobs)
    }
}

#[cfg(windows)]
fn terminate_jobs(jobs: &[windows_sys::Win32::Foundation::HANDLE]) -> anyhow::Result<()> {
    use windows_sys::Win32::System::JobObjects::*;
    for &job in jobs {
        if unsafe { TerminateJobObject(job, 1) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(750);
    loop {
        let mut live = false;
        for &job in jobs {
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
            if unsafe {
                QueryInformationJobObject(
                    job,
                    JobObjectBasicAccountingInformation,
                    &mut info as *mut _ as *mut _,
                    std::mem::size_of_val(&info) as u32,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            live |= info.ActiveProcesses != 0;
        }
        if !live {
            return Ok(());
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "pane jobs did not terminate within 750ms"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(windows)]
impl Drop for ProcessOwner {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}
