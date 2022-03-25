# Lab4: Address Space



`Address Space` is a significant mechanism in modern operating system. By adding a abstract layer between code and physical memory, it frees the developers from the painful memory arrangement work, helping them focus more on their code than the hardware stuff.

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/address-translation.png)

## 0x00 Hardware supports for Multilevel Paging in RISCV

## 0x01 Managing Page table

## 0x02 Address Space of Kernel and User

## 0x03 Multi-tasking with Address Space

__all_traps and trap_return shoud take care of the address space switch. Note that for each task, we should set their TaskContext's initial `ra` to trap_return. We don't have the `ra` pushed in kernel stack for the first time to run a task so we have to handle this manually.

syscall: user space ecall -> __all_traps(trampoline) -> trap_handler -> do syscall -> trap_return -> __restore -> user space

switch: user space -> Interrupt::SupervisorTimer/yield -> __all_traps(trampoline) -> trap_handler -> set_next_trigger&&suspend_current_and_run_next -> schedule -> __switch(change kernel stack) -> trap_return -> __restore -> user space

```rust
// os/src/syscall/process.rs

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
```

```rust
// os/src/syscall/memory.rs

use crate::config::PAGE_SIZE;
use crate::mm::{
    PageTable,
    VirtAddr, 
    MapPermission,
    VPNRange,
};
use crate::task::{
    current_user_token, 
    current_memory_set_mmap, 
    current_memory_set_munmap,
    current_id,
};

pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    if (start & (PAGE_SIZE - 1)) != 0 
        || (prot & !0x7) != 0
        || (prot & 0x7) == 0 {
        return -1;
    }

    let len = ( (len + PAGE_SIZE - 1) / PAGE_SIZE ) * PAGE_SIZE;
    let start_vpn =  VirtAddr::from(start).floor();
    let end_vpn =  VirtAddr::from(start + len).ceil();
    
    let page_table_user = PageTable::from_token(current_user_token());
    // make sure there are no mapped pages in [start..start+len)
    for vpn in VPNRange::new(start_vpn, end_vpn) {
        if let Some(_) = page_table_user.translate(vpn) {
            return -1;
        }
    }
    let mut map_perm = MapPermission::U;
    if (prot & 0x1) != 0 {
        map_perm |= MapPermission::R;
    }
    if (prot & 0x2) !=0 {
        map_perm |= MapPermission::W;
    }
    if (prot & 0x4) !=0 {
        map_perm |= MapPermission::X;
    }

    match current_memory_set_mmap(
        VirtAddr::from(start), 
        VirtAddr::from(start + len), 
        map_perm) {
            Ok(_) => 0,
            Err(e) => {
                error!("[Kernel]: mmap error {}, task id={}", e, current_id());
                -1
            }
    }
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    if (start & (PAGE_SIZE - 1)) != 0 {
        return -1;
    }

    let len = ( (len + PAGE_SIZE - 1) / PAGE_SIZE ) * PAGE_SIZE;
    let start_vpn =  VirtAddr::from(start).floor();
    let end_vpn =  VirtAddr::from(start + len).ceil();

    let page_table_user = PageTable::from_token(current_user_token());
    // make sure there are no unmapped pages in [start..start+len)
    for vpn in VPNRange::new(start_vpn, end_vpn) {
        if let None = page_table_user.translate(vpn) {
            return -1;
        }
    }
    
    current_memory_set_munmap( VirtAddr::from(start), VirtAddr::from(start + len))
}
```