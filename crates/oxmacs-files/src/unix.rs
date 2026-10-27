use std::fs::{self, Metadata, Permissions};
use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use tempfile::Builder;

use crate::write::NewFile;

pub(crate) fn linked(_: &Path, metadata: &Metadata) -> bool {
    metadata.nlink() > 1
}

pub(crate) fn new_file_mode(builder: &mut Builder<'_, '_>, new: NewFile) {
    let mode = match new {
        NewFile::Shared => 0o666,
        NewFile::Private => 0o600,
    };
    builder.permissions(Permissions::from_mode(mode));
}

pub(crate) fn keep_owner(path: &Path, metadata: &Metadata) {
    let _ = std::os::unix::fs::chown(path, Some(metadata.uid()), Some(metadata.gid()));
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
pub(crate) fn copy_attributes(from: &Path, to: &Path) {
    use std::ffi::CString;

    use rustix::fs::{XattrFlags, getxattr, listxattr, setxattr};

    let mut none: [u8; 0] = [];
    let Ok(size) = listxattr(from, &mut none[..]) else {
        return;
    };
    let mut names = vec![0; size];
    let Ok(size) = listxattr(from, &mut names[..]) else {
        return;
    };
    for name in names[..size.min(names.len())]
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        let Ok(name) = CString::new(name) else {
            continue;
        };
        let Ok(length) = getxattr(from, name.as_c_str(), &mut none[..]) else {
            continue;
        };
        let mut value = vec![0; length];
        let Ok(length) = getxattr(from, name.as_c_str(), &mut value[..]) else {
            continue;
        };
        let _ = setxattr(
            to,
            name.as_c_str(),
            &value[..length.min(value.len())],
            XattrFlags::empty(),
        );
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
pub(crate) fn copy_attributes(_: &Path, _: &Path) {}

pub(crate) fn replace(target: &Path, replacement: &Path) -> io::Result<()> {
    fs::rename(replacement, target)
}
