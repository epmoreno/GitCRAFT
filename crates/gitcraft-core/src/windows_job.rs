//! A Windows Job Object holding a child process and everything it starts,
//! so cancelling stops them all (hooks, ssh, credential helpers), as a
//! process group does on Unix.
//!
//! The job does not kill its processes when closed: a program git leaves
//! running on purpose (background maintenance) keeps running, as on Unix.
//!
//! `unsafe` is allowed only in this platform module: the Win32 calls are
//! `unsafe` in the `windows` crate. Each block says why it is sound.
#![allow(unsafe_code)]

use std::os::windows::io::RawHandle;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
};
use windows::core::PCWSTR;

/// A job that owns its handle.
#[derive(Debug)]
pub struct Job(HANDLE);

// SAFETY: a job handle is a kernel handle, usable from any thread; `Job`
// only passes it to thread-safe Win32 calls.
unsafe impl Send for Job {}
// SAFETY: as above; no call mutates `Job` itself.
unsafe impl Sync for Job {}

impl Job {
    /// A new job holding `process`. `None` if Windows refuses (the process
    /// then stops on its own, as before).
    pub fn holding(process: RawHandle) -> Option<Self> {
        // SAFETY: an unnamed job with default security; the handle is owned
        // by the returned `Job` and closed once, on drop.
        let job = Self(unsafe { CreateJobObjectW(None, PCWSTR::null()) }.ok()?);
        // SAFETY: `job.0` is the valid handle just created and `process` is
        // the handle of a child that has not been waited on, so still open.
        unsafe { AssignProcessToJobObject(job.0, HANDLE(process)) }.ok()?;
        Some(job)
    }

    /// Ends every process in the job.
    pub fn terminate(&self) {
        // SAFETY: `self.0` is a valid job handle owned by `self`. An error
        // means the processes are already gone.
        let _ = unsafe { TerminateJobObject(self.0, 1) };
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by `self` and closed exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}
