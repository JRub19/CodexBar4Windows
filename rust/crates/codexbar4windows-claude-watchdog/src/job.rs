//! Windows Job Object wrapper. A `Job` holds a kernel handle that all
//! child processes are assigned to; when the handle goes out of scope
//! the kernel kills every member of the job. We use
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so the watchdog crashing or
//! exiting cleanly both reap the Claude CLI child.

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::mem::size_of;

    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub struct Job {
        handle: HANDLE,
    }

    impl Job {
        pub fn create() -> std::io::Result<Self> {
            unsafe {
                let handle = CreateJobObjectW(None, None).map_err(std::io::Error::other)?;
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const c_void,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
                .map_err(std::io::Error::other)?;
                Ok(Self { handle })
            }
        }

        pub fn assign(&self, process: HANDLE) -> std::io::Result<()> {
            unsafe { AssignProcessToJobObject(self.handle, process).map_err(std::io::Error::other) }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(windows)]
pub use platform::Job;

#[cfg(not(windows))]
pub struct Job;

#[cfg(not(windows))]
impl Job {
    pub fn create() -> std::io::Result<Self> {
        // The watchdog only runs on Windows; the non-Windows stub exists
        // so the crate compiles on Linux CI.
        Err(std::io::Error::other("watchdog is windows only"))
    }
}
