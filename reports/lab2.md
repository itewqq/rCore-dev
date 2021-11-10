# Lab2: Batch Processing and Privileges

In lab 1, we have made our code work on a **bare-metal computer** (simulated by QEMU) successfully. However, it can do nothing but print some strings we hardcoded in the program on the terminal. Of course you can make it more complicated, such as factoring a large number, calculating the inverse of a matrix, etc. That's cool but there are two significant drawbacks of this approach:

1. The CPU runs a single program each time. Since the computing resources are precious(especially in the old time when you don't have a modern OS), users who have many programs to run have to wait in front of the computer and manually load&start the next program after the previous one finished.
2. Nobody wants to write the SBI and assembly level stuff every time, and it's a duplication of efforts.

In order to solve these problems, people invented the `Simple Batch Processing System`, which can load a batch of application programs and automatically execute them one by one. Besides, the Batch Processing System will provide some "library" code such as console output functions which may be reused by many programs. 

A new problem arises when we use the batch process system: error handling. The user's program may (often) run into errors, unconsciously or intentionally. We do not want the error of any program to affect others or the system, so the system should be able to handle these errors and terminate the programs when necessary. To achieve this goal we introduced the `Privileges mechanism` and isolate user's code from the system, which we will refer to as `user mode` and `kernel mode`. Note that this mechanism requires some support from hardware, and we will illustrate that with code in the following parts.

## 0x00 Privileges mechanism

The underlying reason for implementing the privileges mechanism is the system cannot trust any submitted program. Any errors or attacks could happen and may corrupt the system. We have to restrict users' programs in an isolated "harmless" environment, where they have no access to 1) arbitrary memory or 2) any over-powerful instructions which may break the computer. In this lab, we mainly focus on the last point.

Prohibiting users' program from using privileged instructions need the help from CPU. In riscv64, 4 levels of privileges are designed:

| Level | Encode |         Name        |
|:-----:|:------:|:-------------------:|
|   0   |   00   | U, User/Application |
|   1   |   01   |    S, Supervisor    |
|   2   |   10   |    H, Hypervisor    |
|   3   |   11   |      M, Machine     |

All modes, except `Machine`, have to go through binary interfaces provided by higher levels to control the hardware. The privileges level and their relation in our scenario are shown in the following figure:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/PrivilegeStack.png)

The binary interfaces between User mode and Supervisor mode are named Application Binary Interface (ABI), or another more famous one: `syscall`. 

Each time when a user mode app want to access hardware resources(e.g., print a line on the screen), the following sequence will take place:

1. The app uses `ecall` instruction to trigger a `trap`, which will cause the CPU to elevate the current privilege level and jump to the `trap handler` function set in the `stvec` register. 

```rust
global_asm!(include_str!("trap.S"));

pub fn init(){
    extern "C" {fn __alltraps();}
    unsafe {
        stvec::write(__alltraps as usize, TrapMode::Direct); // set the entry for trap handler
    }
}
```

2. The `trap handler` in the OS will first store the context of the app, then handles the trap according to its parameters. We implemented this in `./os/src/trap/trap.S` 

```assembly
.altmacro
.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm
.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm
    .section .text
    .globl __alltraps
    .globl __restore
    .align 2
__alltraps:
    csrrw sp, sscratch, sp
    # now sp->kernel stack, sscratch->user stack
    # allocate a TrapContext on kernel stack
    addi sp, sp, -34*8
    # save general-purpose registers
    sd x1, 1*8(sp)
    # skip sp(x2), we will save it later
    sd x3, 3*8(sp)
    # skip tp(x4), application does not use it
    # save x5~x31
    .set n, 5
    # need .altmacro marco
    .rept 27
        SAVE_GP %n
        .set n, n+1
    .endr
    # we can use t0/t1/t2 freely, because they were saved on kernel stack
    csrr t0, sstatus
    csrr t1, sepc
    sd t0, 32*8(sp)
    sd t1, 33*8(sp)
    # read user stack from sscratch and save it on the kernel stack
    csrr t2, sscratch
    sd t2, 2*8(sp)
    # set input argument of trap_handler(cx: &mut TrapContext)
    mv a0, sp
    call trap_handler
```

3. In `trap handler` we handle the traps according to their type, or just terminate it and run the next app if we think it's doing something bad.

```rust
#[no_mangle]
pub fn trap_handler(ctx: &mut TrapContext) -> &mut TrapContext {
    let scause = scause::read();
    let stval = stval::read();
    // println!("{:?}", scause.cause());
    match scause.cause() {
        Trap::Exception(Exception::UserEnvCall) => {
            ctx.sepc += 4; // add bytecode length to point to ecall's next instruction
            ctx.x[10] = syscall(ctx.x[17], [ctx.x[10], ctx.x[11], ctx.x[12]]) as usize; // a0-a2
        }
        // handle other Exception
        Trap::Exception(Exception::StoreFault) |
        Trap::Exception(Exception::StorePageFault) => {
            println!("[kernel] PageFault in application, core dumped.");
            run_next_app();
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            println!("[kernel] IllegalInstruction in application, core dumped.");
            run_next_app();
        }
        _ => {
            println!("Unsupported trap {:?}, stval = {:#x}!", scause.cause(), stval);
            run_next_app();
        }
    }
    ctx
}
```

4. After the desired operations have been executed, the OS will recover the context of the user mode app. Then the OS uses `sret` instruction to make the CPU reduce the privilege level to user mode and jump back to the next line of the `ecall` in step 1.

```assembly
__restore:
    # case1: start running app by __restore
    # case2: back to U after handling trap
    mv sp, a0
    # now sp->kernel stack(after allocated), sscratch->user stack
    # restore sstatus/sepc
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    ld t2, 2*8(sp)
    csrw sstatus, t0
    csrw sepc, t1
    csrw sscratch, t2
    # restore general-purpuse registers except sp/tp
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr
    # release TrapContext on kernel stack
    addi sp, sp, 34*8
    # now sp->user stack, sscratch->kernel stack
    csrrw sp, sscratch, sp
    sret
```

5. Then the user mode app continue to run.

The interaction between mode S and mode M is similar to the above one, so generally the privilege level switching can be illustrated like this:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/EnvironmentCallFlow.png)

Also, the OS has the power to terminate a user mode program when necessary (e.g. the app tries to use some privilege instruction like `sret`).

>The associated code was placed in the `./os/trap` directory.

## 0x01 Batch Processing System

With the help of privileges, we can safely implement a batch processing system. The basic idea is straightforward: each time we load a binary to address 0x80400000 and jump there to execute. We have no file system yet, hence the applications have to be stored in the OS's binary at compile time. Our solution is to generate a `link_app.S` includeing all the application binary files:

```assembly

    .align 3
    .section .data
    .global _num_app
_num_app:
    .quad 11
    .quad app_0_start
    .quad app_1_start
    .quad app_2_start
    .quad app_3_start
    .quad app_4_start
    .quad app_5_start
    .quad app_6_start
    .quad app_7_start
    .quad app_8_start
    .quad app_9_start
    .quad app_10_start
    .quad app_10_end

    .section .data
    .global app_0_start
    .global app_0_end
app_0_start:
    .incbin "../user/target/riscv64gc-unknown-none-elf/release/00hello_world.bin"
app_0_end:

    .section .data
    .global app_1_start
    .global app_1_end
app_1_start:
    .incbin "../user/target/riscv64gc-unknown-none-elf/release/01store_fault.bin"
app_1_end:

    .section .data
    .global app_2_start
    .global app_2_end
app_2_start:
    .incbin "../user/target/riscv64gc-unknown-none-elf/release/02power.bin"
app_2_end:

    .section .data
    .global app_3_start
    .global app_3_end
app_3_start:
    .incbin "../user/target/riscv64gc-unknown-none-elf/release/03_ch2_bad_instruction.bin"
app_3_end:

    .section .data
    .global app_4_start
    .global app_4_end
app_4_start:
    .incbin "../user/target/riscv64gc-unknown-none-elf/release/04_ch2_bad_register.bin"
app_4_end:

    .section .data
    .global app_5_start
    .global app_5_end
app_5_start:
    .incbin "../user/target/riscv64gc-unknown-none-elf/release/05_ch2t_bad_address.bin"
app_5_end:

    .section .data
    .global app_6_start
    .global app_6_end
app_6_start:
    .incbin "../user/target/riscv64gc-unknown-none-elf/release/06ch2_exit.bin"
app_6_end:

    .section .data
    .global app_7_start
    .global app_7_end
app_7_start:
    .incbin "../user/target/riscv64gc-unknown-none-elf/release/07ch2_hello_world.bin"
app_7_end:

    .section .data
    .global app_8_start
    .global app_8_end
app_8_start:
    .incbin "../user/target/riscv64gc-unknown-none-elf/release/08ch2_power.bin"
app_8_end:

    .section .data
    .global app_9_start
    .global app_9_end
app_9_start:
    .incbin "../user/target/riscv64gc-unknown-none-elf/release/09ch2_write1.bin"
app_9_end:

    .section .data
    .global app_10_start
    .global app_10_end
app_10_start:
    .incbin "../user/target/riscv64gc-unknown-none-elf/release/10ch2t_write0.bin"
app_10_end:
```

And then we include the assembly file in `./os/src/main.rs`:

```rust
global_asm!(include_str!("link_app.S"));
```

The code to generate `link_app.S` is `./os/build.rs`.

Then, for each application we take the following operations:

1. Erase the memory from 0x80400000 to 0x80400000+0x20000, then load the target binary to 0x80400000 (we assume that it's size < 0x20000 Bytes)

```rust
// os/src/batch.rs
unsafe fn load_app(&self, app_id: usize) {
    if app_id >= self.num_app {
        panic!("All applications completed!");
    }
    println!("[kernel] Loading app{} ...", app_id);
    asm!("fence.i"); // clear i-cache
    for addr in APP_BASE_ADDRESS..APP_BASE_ADDRESS+APP_SIZE_LIMIT {
        (addr as *mut u8).write_volatile(0);
    }
    let src = core::slice::from_raw_parts(
        (self.app_start_addrs[app_id]) as *const u8, 
        self.app_start_addrs[app_id+1]-self.app_start_addrs[app_id]);
    let dst = core::slice::from_raw_parts_mut(APP_BASE_ADDRESS as *mut u8,
        self.app_start_addrs[app_id+1]-self.app_start_addrs[app_id]);
    dst.copy_from_slice(src);
}
```

2. Initialize the registers and stack pointer, set the `sepc` to the entry address 0x80400000.

```rust
// os/src/trap/context.rs
pub fn app_init_context(entry: usize, sp: usize) -> Self {
    let mut sstatus = sstatus::read();
    sstatus.set_spp(SPP::User);
    let mut cx = Self {
        x: [0; 32],
        sstatus,
        sepc: entry,
    };
    cx.set_sp(sp);
    cx
}
```

3. Reuse `__restore` to fire up a application and set the kernel stack at the same time.

```rust
// os/src/batch.rs
extern "C" { fn __restore(cx_addr: usize); }
// execute it with sret in __restore
unsafe {
    __restore(KERNEL_STACK.push_context(
        TrapContext::app_init_context(APP_BASE_ADDRESS, USER_STACK.get_sp())
    ) as *const _ as usize);
}
```

4. After an application finished, move to the next app.

```rust
// os/src/batch.rs
pub fn run_next_app() -> ! {
    let current_app = APP_MANAGER.inner.borrow().get_current_app();
    unsafe {
        APP_MANAGER.inner.borrow().load_app(current_app);
    }
    APP_MANAGER.inner.borrow_mut().move_to_next_app();
    extern "C" { fn __restore(cx_addr: usize); }
    // execute it with sret in __restore
    unsafe {
        __restore(KERNEL_STACK.push_context(
            TrapContext::app_init_context(APP_BASE_ADDRESS, USER_STACK.get_sp())
        ) as *const _ as usize);
    }
    panic!("Unreachable in batch::run_current_app!");
}

// ./os/src/syscall/process.rs
use crate::batch::run_next_app;

pub fn sys_exit(exit_code: i32) -> ! {
    println!("[kernel] Application exited with code {}", exit_code);
    run_next_app()
}
```

## 0x02 Basic Security Checks

Currently, we only provide two `syscall`s for the user mode applications. As the `sys_write` may write to some sensitive addresses, we need to add some security checks. Here we verify whether the target interval covers addresses other than the user stack space and the app's storage space.

```rust
use crate::batch::{APP_BASE_ADDRESS, APP_SIZE_LIMIT, APP_MANAGER, USER_STACK, USER_STACK_SIZE};

const FD_STDOUT: usize = 1;

pub fn sys_write(fd: usize, buffer: *const u8, len: usize) -> isize {
    match fd {
        FD_STDOUT => {
            let sp_top = USER_STACK.get_sp();
            let sp_bottom = sp_top - USER_STACK_SIZE;
            let current_app_size = APP_MANAGER.inner.borrow().get_current_app_size();
            let app_size = core::cmp::min(APP_SIZE_LIMIT,current_app_size);

            if  !(  (buffer >= APP_BASE_ADDRESS as *const u8 && buffer <= (APP_BASE_ADDRESS + app_size) as *const u8 )
                  ||(buffer >= sp_bottom as *const u8 && buffer <= sp_top as *const u8)){
                error!("Illegal Address detected!");
                return -1;
            }

            unsafe {
                if  !(  (buffer.offset(len as isize) >= APP_BASE_ADDRESS as *const u8 && buffer.offset(len as isize) <= (APP_BASE_ADDRESS + app_size) as *const u8 )
                    ||(buffer.offset(len as isize) >= sp_bottom as *const u8 && buffer.offset(len as isize) <= sp_top as *const u8)){
                    error!("Illegal Address detected!");
                    return -1;
                }
            }

            let raw_bytes = unsafe { core::slice::from_raw_parts(buffer, len) };
            let str = core::str::from_utf8(raw_bytes).unwrap();
            print!("{}", str);
            len as isize
        },
        _ => {
            error!("Unsupported fd type: {} for sys_write", fd);
            -1 as isize
        }
    }
}
```

## 0x03 Final Result

Run `make run` in the `os` directory:

```bash
Platform: qemu
   Compiling os v0.1.0 (/home/qsp/rCore-dev/os)
    Finished release [optimized] target(s) in 0.41s
[rustsbi] RustSBI version 0.2.0-alpha.6
.______       __    __      _______.___________.  _______..______   __
|   _  \     |  |  |  |    /       |           | /       ||   _  \ |  |
|  |_)  |    |  |  |  |   |   (----`---|  |----`|   (----`|  |_)  ||  |
|      /     |  |  |  |    \   \       |  |      \   \    |   _  < |  |
|  |\  \----.|  `--'  |.----)   |      |  |  .----)   |   |  |_)  ||  |
| _| `._____| \______/ |_______/       |__|  |_______/    |______/ |__|

[rustsbi] Implementation: RustSBI-QEMU Version 0.0.2
[rustsbi-dtb] Hart count: cluster0 with 1 cores
[rustsbi] misa: RV64ACDFIMSU
[rustsbi] mideleg: ssoft, stimer, sext (0x222)
[rustsbi] medeleg: ima, ia, bkpt, la, sa, uecall, ipage, lpage, spage (0xb1ab)
[rustsbi] pmp0: 0x10000000 ..= 0x10001fff (rwx)
[rustsbi] pmp1: 0x80000000 ..= 0x8fffffff (rwx)
[rustsbi] pmp2: 0x0 ..= 0xffffffffffffff (---)
qemu-system-riscv64: clint: invalid write: 00000004
[rustsbi] enter supervisor 0x80200000
[kernel] Hello, world!
Number of app is 11
[kernel] app 0 starts at 0x8020a068, ends at 0x8020b070
[kernel] app 1 starts at 0x8020b070, ends at 0x8020c108
[kernel] app 2 starts at 0x8020c108, ends at 0x8020d2b8
[kernel] app 3 starts at 0x8020d2b8, ends at 0x8020e298
[kernel] app 4 starts at 0x8020e298, ends at 0x8020f3f8
[kernel] app 5 starts at 0x8020f3f8, ends at 0x802103d0
[kernel] app 6 starts at 0x802103d0, ends at 0x802113b8
[kernel] app 7 starts at 0x802113b8, ends at 0x802123e8
[kernel] app 8 starts at 0x802123e8, ends at 0x80213598
[kernel] app 9 starts at 0x80213598, ends at 0x80214c20
[kernel] app 10 starts at 0x80214c20, ends at 0x802161d8
[kernel] Loading app0 ...
Hello, world!
[kernel] IllegalInstruction in application, core dumped.
[kernel] Loading app1 ...
Into Test store_fault, we will insert an invalid store operation...
Kernel should kill this application!
[kernel] PageFault in application, core dumped.
[kernel] Loading app2 ...
3^10000=5079
3^20000=8202
3^30000=8824
3^40000=5750
3^50000=3824
3^60000=8516
3^70000=2510
3^80000=9379
3^90000=2621
3^100000=2749
Test power OK!
[kernel] Application exited with code 0
[kernel] Loading app3 ...
[kernel] IllegalInstruction in application, core dumped.
[kernel] Loading app4 ...
[kernel] IllegalInstruction in application, core dumped.
[kernel] Loading app5 ...
[kernel] PageFault in application, core dumped.
[kernel] Loading app6 ...
[kernel] Application exited with code 1234
[kernel] Loading app7 ...
Hello world from user mode program!
Test hello_world OK!
[kernel] Application exited with code 0
[kernel] Loading app8 ...
3^10000=5079
3^20000=8202
3^30000=8824
3^40000=5750
3^50000=3824
3^60000=8516
3^70000=2510
3^80000=9379
3^90000=2621
3^100000=2749
Test power OK!
[kernel] Application exited with code 0
[kernel] Loading app9 ...
Unsupported fd type: 1234 for sys_write
string from data section
strinstring from stack section
strin
Test write1 OK!
[kernel] Application exited with code 0
[kernel] Loading app10 ...
Illegal Address detected!
Illegal Address detected!
Illegal Address detected!
Test write0 OK!
[kernel] Application exited with code 0
Panicked at src/batch.rs:74 All applications completed!
```

Note that we add malformed code in some apps and the security checks find them successfully, check the code in `./user/src/bin`.

## References

https://rcore-os.github.io/rCore-Tutorial-Book-v3/chapter2/index.html

>All of the figures credit to rCore-Tutorial-Book-v3