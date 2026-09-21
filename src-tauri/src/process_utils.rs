use std::process::Command;
use std::ffi::OsStr;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

pub fn command<S: AsRef<OsStr>>(program: S) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);
    cmd
}
