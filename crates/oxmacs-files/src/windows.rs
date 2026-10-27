use std::ffi::OsStr;
use std::fs::{self, File, Metadata};
use std::io;
use std::iter;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use tempfile::Builder;
use windows_sys::Win32::Storage::FileSystem::{
    REPLACEFILE_IGNORE_ACL_ERRORS, REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW,
};

use crate::write::NewFile;

pub(crate) fn linked(path: &Path, _: &Metadata) -> bool {
    File::open(path)
        .and_then(|file| winapi_util::file::information(&file))
        .is_ok_and(|information| information.number_of_links() > 1)
}

pub(crate) fn new_file_mode(_: &mut Builder<'_, '_>, _: NewFile) {}

pub(crate) fn keep_owner(_: &Path, _: &Metadata) {}

pub(crate) fn copy_attributes(_: &Path, _: &Path) {}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(iter::once(0)).collect()
}

#[expect(unsafe_code, reason = "ReplaceFileW has no safe wrapper")]
fn replace_file(target: &Path, replacement: &Path) -> io::Result<()> {
    let (target, replacement) = (wide(target.as_os_str()), wide(replacement.as_os_str()));
    let done = unsafe {
        ReplaceFileW(
            target.as_ptr(),
            replacement.as_ptr(),
            ptr::null(),
            REPLACEFILE_IGNORE_MERGE_ERRORS | REPLACEFILE_IGNORE_ACL_ERRORS,
            ptr::null(),
            ptr::null(),
        )
    };
    if done == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(crate) fn replace(target: &Path, replacement: &Path) -> io::Result<()> {
    replace_file(target, replacement).or_else(|_| fs::rename(replacement, target))
}
