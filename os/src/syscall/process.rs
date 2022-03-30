use core::mem::size_of;

use crate::task::{
    current_task,
    current_user_token,
    add_task,
    suspend_current_and_run_next,
};
use crate::loader::get_app_data_by_name;
use crate::mm::{translated_byte_buffers, translated_str};
use crate::timer::get_time_us;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    kprintln!("Application {} exited with code {}", current_id(), exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_set_priority(prio: usize) -> isize {
    if (prio as isize) < 2 {
        -1
    } else{
        set_current_prio(prio);
        prio as isize
    }
}

pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    let len = size_of::<TimeVal>();
    let mut ts_buffers = translated_byte_buffers(current_user_token(), ts.cast(), len);
    // At least one buf
    if ts_buffers.len() <= 0 {
        return -1;
    }
    let us = get_time_us();
    let ts: *mut TimeVal = ts_buffers[0].as_mut_ptr().cast();
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}

pub fn sys_fork() -> isize {
    let current = current_task().unwrap();
    let child_task = current.fork();
    let child_pid = child_task.pid.0;
    // modify return address in trap context
    let trap_cx = child_task.inner_exclusive_access().get_trap_cx();
    // return value of child is 0
    trap_cx.x[10] = 0;  //x[10] is a0 reg
    // add task to scheduler queue
    add_task(child_task);
    child_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}