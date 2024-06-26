// SPDX-License-Identifier: MPL-2.0

use super::SyscallReturn;
use crate::{
    prelude::*,
    process::{wait_child_exit, ProcessFilter, WaitOptions},
    util::write_val_to_user,
};

pub fn sys_wait4(wait_pid: u64, exit_status_ptr: u64, wait_options: u32) -> Result<SyscallReturn> {
    let wait_options = WaitOptions::from_bits(wait_options)
        .ok_or_else(|| Error::with_message(Errno::EINVAL, "unknown wait option"))?;
    debug!(
        "pid = {}, exit_status_ptr = {}, wait_options: {:?}",
        wait_pid as i32, exit_status_ptr, wait_options
    );
    debug!("wait4 current pid = {}", current!().pid());
    let process_filter = ProcessFilter::from_id(wait_pid as _);
    let (return_pid, exit_code) = wait_child_exit(process_filter, wait_options)?;
    if return_pid != 0 && exit_status_ptr != 0 {
        write_val_to_user(exit_status_ptr as _, &exit_code)?;
    }
    Ok(SyscallReturn::Return(return_pid as _))
}
