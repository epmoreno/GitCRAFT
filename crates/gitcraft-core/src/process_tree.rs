//! Stopping a command together with everything it started (hooks, ssh,
//! credential helpers), on every system.
//!
//! - Unix: the command gets its own process group, signalled with `killpg`:
//!   SIGTERM first, so git can clean up, then SIGKILL.
//! - Windows: the command joins a Job Object as soon as it starts, and
//!   `TerminateJobObject` ends all of it. Console programs have no polite
//!   "please stop" there that a windowless app can send, so stopping is
//!   immediate. Commands start without a console window (`CREATE_NO_WINDOW`).

use tokio::process::{Child, Command};

/// Prepares `cmd` so its process tree can be stopped later.
pub fn prepare(cmd: &mut Command) {
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    {
        /// `CREATE_NO_WINDOW` (winbase.h): no console window flashes up.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(any(unix, windows)))]
    let _ = cmd;
}

/// The processes of one command.
#[derive(Debug)]
pub struct ProcessTree {
    #[cfg(unix)]
    group: Option<i32>,
    #[cfg(windows)]
    job: Option<crate::windows_job::Job>,
}

impl ProcessTree {
    /// Whether [`terminate`](Self::terminate) asks politely (and
    /// [`kill`](Self::kill) is still needed after a grace period).
    pub const POLITE_STOP: bool = cfg!(unix);

    /// Takes hold of the tree of `child`, just spawned from a command made
    /// with [`prepare`].
    pub fn adopt(child: &Child) -> Self {
        Self {
            #[cfg(unix)]
            group: child.id().and_then(|id| i32::try_from(id).ok()),
            #[cfg(windows)]
            job: child
                .raw_handle()
                .and_then(crate::windows_job::Job::holding),
        }
    }

    /// Asks the tree to stop (SIGTERM on Unix; ends it on Windows).
    pub fn terminate(&self) {
        #[cfg(unix)]
        self.signal(nix::sys::signal::Signal::SIGTERM);
        #[cfg(windows)]
        self.end();
    }

    /// Ends the tree now.
    pub fn kill(&self) {
        #[cfg(unix)]
        self.signal(nix::sys::signal::Signal::SIGKILL);
        #[cfg(windows)]
        self.end();
    }

    #[cfg(unix)]
    fn signal(&self, signal: nix::sys::signal::Signal) {
        if let Some(group) = self.group {
            // ESRCH just means the group is already gone.
            let _ = nix::sys::signal::killpg(nix::unistd::Pid::from_raw(group), signal);
        }
    }

    #[cfg(windows)]
    fn end(&self) {
        if let Some(job) = &self.job {
            job.terminate();
        }
    }
}
