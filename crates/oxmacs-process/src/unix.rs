use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

use rustix::process::{Pid, Signal, kill_process_group, setsid};

pub(crate) fn hide_console(_: &mut Command) {}

#[expect(unsafe_code, reason = "pre_exec has no safe form")]
pub(crate) fn prepare(command: &mut Command) {
    let alone = || setsid().map(drop).map_err(io::Error::from);
    unsafe {
        command.pre_exec(alone);
    }
}

pub(crate) fn ask_to_end(tree: &Tree) {
    let _ = kill_process_group(tree.0, Signal::TERM);
}

#[derive(Debug)]
pub(crate) struct Tree(Pid);

impl Tree {
    pub(crate) fn of(child: &Child) -> Self {
        Self(Pid::from_child(child))
    }

    pub(crate) fn kill(&self) -> bool {
        kill_process_group(self.0, Signal::KILL).is_ok()
    }
}
