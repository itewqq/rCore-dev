use core::mem::size_of;

use crate::fs::{linkat, open_file, unlinkat, OpenFlags, Stat};
use crate::mm::{translated_byte_buffers, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};

const AT_FDCWD: i32 = -100;

const FD_STDIN: usize = 0;
const FD_STDOUT: usize = 1;

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffers(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.read(UserBuffer::new(translated_byte_buffers(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    // overflow
    if fd >= inner.fd_table.len() {
        return -1;
    }
    // is busy
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

pub fn sys_linkat(
    olddirfd: i32,
    oldpath: *const u8,
    newdirfd: i32,
    newpath: *const u8,
    flags: u32,
) -> isize {
    assert_eq!(olddirfd, AT_FDCWD);
    assert_eq!(newdirfd, AT_FDCWD);
    assert_eq!(flags, 0);
    let token = current_user_token();
    let oldpath = translated_str(token, oldpath);
    let newpath = translated_str(token, newpath);
    linkat(&oldpath, &newpath, flags)
}

pub fn sys_unlinkat(dirfd: i32, path: *const u8, flags: u32) -> isize {
    assert_eq!(dirfd, AT_FDCWD);
    assert_eq!(flags, 0);
    let token = current_user_token();
    let path = translated_str(token, path);
    unlinkat(&path, flags)
}

pub fn sys_fstat(fd: usize, st: *mut Stat) -> isize {
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    // translate buffer in user mode
    let len = size_of::<Stat>();
    // release current task TCB manually to avoid multi-borrow
    drop(inner);
    let mut ts_buffers = translated_byte_buffers(current_user_token(), st.cast(), len);
    // At least one buf
    if ts_buffers.len() <= 0 {
        return -1;
    }
    let st: *mut Stat = ts_buffers[0].as_mut_ptr().cast();
    // re-access to current task TCB
    let inner = task.inner_exclusive_access();
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        let stat = file.fstat();
        unsafe {
            *st = stat;
        }
        0
    } else {
        -1
    }
}
