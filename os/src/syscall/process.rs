use core::mem::size_of;

use crate::task::{
    suspend_current_and_run_next,
    exit_current_and_run_next,
    set_current_prio,
    current_user_token,
    current_id,
};
use crate::mm::{PageTable, translated_byte_buffers, VirtPageNum, VirtAddr, PhysAddr};
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