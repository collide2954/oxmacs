use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};
use std::ptr;

use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_LIMIT_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn hide_console(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

pub(crate) fn prepare(_: &mut Command) {}

pub(crate) fn ask_to_end(_: &Tree) {}

#[derive(Debug)]
pub(crate) struct Tree(Option<OwnedHandle>);

impl Tree {
    pub(crate) fn of(child: &Child) -> Self {
        Self(job_for(child))
    }

    pub(crate) fn kill(&self) -> bool {
        self.0.as_ref().is_some_and(end)
    }
}

#[expect(unsafe_code, reason = "Job Objects have no safe wrapper")]
fn job_for(child: &Child) -> Option<OwnedHandle> {
    let limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
            LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK,
            ..JOBOBJECT_BASIC_LIMIT_INFORMATION::default()
        },
        ..JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default()
    };
    let size = u32::try_from(size_of_val(&limits)).ok()?;
    unsafe {
        let job = CreateJobObjectW(ptr::null(), ptr::null());
        if job.is_null() {
            return None;
        }
        let job = OwnedHandle::from_raw_handle(job);
        let ready = SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size,
        ) != 0
            && AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) != 0;
        ready.then_some(job)
    }
}

#[expect(unsafe_code, reason = "Job Objects have no safe wrapper")]
fn end(job: &OwnedHandle) -> bool {
    unsafe { TerminateJobObject(job.as_raw_handle(), 1) != 0 }
}
