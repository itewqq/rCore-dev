# Lab4: Address Space

`Address Space` is a significant mechanism in modern operating system. By adding an abstract layer between code and physical memory, it frees the developers from the painful memory arrangement work, helping them focus more on their code other than the hardware stuff.

The following figure gives an overview of how `Address Space` works:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/address-translation.png)

Having `Address Space` enabled, the codes can only see the `Virtual Address`. If a process wants to access any address `virt_addr`, it will be first translated to `Physical Address` by CPU's MMU module according to the process's page table.

## 0x00 Hardware supports for Multilevel Paging in RISCV

The MMU is disabled by default, thus previously any program are able to access any physical memory. We can enbale the MMU by setting a register named `satp`:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/satp.png)

The above figure shows the meaning of bits in `satp`:

- `MODE` controls how the MMU translate address. When `MODE` = 0 the MMU is disabled, and when 'MODE' = 8 the MMU use page table mechanism to translate the address.
- `ASID` indetifies the address space by id, since we don't have process implemented yet we just ignore it.
- `PPN` is the physical page number of the root page table entry.

The address format under page table mechanism consists of two parts: page number and offset:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/sv39-va-pa.png)

Each page table entry consists of 3 level virtual page number (`vpn`) and several flag bits:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/sv39-pte.png)

With these knowledge we can easily understand how the MMU translates virtual memory address:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/sv39-full.png)

### TLB

TLB (Translation Lookaside Buffer) works like some kinds of cache, note that we have to use `sfence.vma` instruction to refresh it after we change `satp` or any page table entry.

## 0x01 Address Space of Kernel and User

After `satp` is enabled, the memory of kernel and user applications are seperated, we need to carefully handle the interaction between different address spaces. In rCore, the designers use a `Trampoline` to bridge the kernel and usermode applications:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/kernel-as-high.png)

The virtual address of `Trampoline` is exactly same across each user spaces and the kernel space. Note that there is a `guard page` between kernel stacks. Those `hole`s in the address space are settled to prevent buffer overflow damage in kernel stack. 

The address space of the kernel is illustrated in the following figure:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/kernel-as-low.png)

The permissions here are critical for system security: no page table can be both writable and executable. Besides, we use identical mapping here, so the kernel can read/write any user space memory in an easy way.

In user mode, the address space is quite familiar to us:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/app-as-full.png)

We palce the `TrapContext` just under the `Trampoline`.

## 0x02 Multi-tasking with Address Space

__all_traps and trap_return shoud take care of the address space switch. Note that for each task, we should set their TaskContext's initial `ra` to trap_return. We don't have the `ra` pushed in kernel stack for the first time to run a task so we have to handle this manually.

The `syscall` call stack is:

```
syscall: user space ecall -> __all_traps(trampoline) -> trap_handler -> do syscall -> trap_return -> __restore -> user space
```

The `switch` process is:

```
switch: user space -> Interrupt::SupervisorTimer/yield -> __all_traps(trampoline) -> trap_handler -> set_next_trigger&&suspend_current_and_run_next -> schedule -> __switch(change kernel stack) -> trap_return -> __restore -> user space
```

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