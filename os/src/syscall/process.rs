use alloc::sync::Arc;
use core::mem::size_of;

use crate::task::{
    current_task,
    current_user_token,
    set_current_prio,
    add_task,
    suspend_current_and_run_next,
    exit_current_and_run_next,
};
use crate::fs::{open_file, OpenFlags};
use crate::mm::{translated_byte_buffers, translated_str, translated_refmut};
use crate::timer::get_time_us;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    exit_current_and_run_next(exit_code);
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

pub fn sys_getpid() -> isize {
    current_task().unwrap().pid.0 as isize
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
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

// If pid == -1, try to recycle every child
// If there is not a child process whose pid is same as given, return -1.
// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    // no such child
    if inner.children
        .iter()
        .find(|p| {pid == -1 || pid as usize == p.getpid()})
        .is_none() {
        return -1;
    }
    // get child
    let pair = inner.children
        .iter()
        .enumerate()
        .find(|(_, p)| {
            // ++++ temporarily access child PCB exclusively
            p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
            // ++++ stop exclusively accessing child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after removing from children list
        // so that after dropped, the kernel stack and pagetable and pid_handle will be recycled
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        let exit_code = child.inner_exclusive_access().exit_code;
        // write exit_code to the user space
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
}