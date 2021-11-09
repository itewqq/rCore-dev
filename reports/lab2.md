# Lab2: Batch Processing and Priviledges

In lab 1, we have made our code work on a **bare-metal computer** (simulated by QEMU) successfully. However, it can do nothing but print some strings we hardcoded in the program on the terminal. Of course you can make it more complicated, such as factoring a large number, calculating the inverse of a matrix, etc. That's cool but there are two significant drawbacks of this approach:

1. The CPU runs a single program each time. Since the computing resuorces are precious(especially in the old time when you don't have a modern OS), users who have many programs to run have to wait infront of the computer and manully load&start the next program after previous one finished. Such a labour work.
2. Nobody wants to write the SBI and assembly level stuff everytime, and it's totally duplication of efforts.

In order to solve these problems, people invented the `Simple Batch Processing System`, which can load a batch of application programs and automatically execute them one by one. Besides, the Batch Processing System will provide some "library" code such as console output functions which may be reused by many programs. 

A new problem arises when we use the batch process system: error handling. User's program may (often) run into errors, unconsciously or intentionally. We do not want the error of any program affect others or the system, so the system should be able to hanble errors and terminate the programs if necessary. To achieve this goal we introduced the `Priviledges mechanism` and isolate user's code from system, which we will refered to as `usermode` and `kernelmode`. Note that this mechanism requires some support from hardware, and we will illustrate that with code in the following parts.

## 0x00 Priviledges mechanism

The underlying reason for implementing the priviledges mechanism is the system cannot trust any submitted program. Any errors or attacks could happen and may corrupt the system. We have to restrict users' programs in an isolated "harmless" environment, where they have no access to 1) arbitrary memory or 2) any over-powerful instructions which may break the computer. In this lab we mainly focus on the last point.

Obviously, prohibiting users' program from using priviledged instructions need the help from processor. In riscv64, 4 levels of priviledges are designed:

| Level | Encode |         Name        |
|:-----:|:------:|:-------------------:|
|   0   |   00   | U, User/Application |
|   1   |   01   |    S, Supervisor    |
|   2   |   10   |    H, Hypervisor    |
|   3   |   11   |      M, Machine     |

All modes, except `Machine` mode, have to go through binary interfaces provided by higer modes if they want to control the hardware. The priviledges level and their relation in our scenario are shown in the following figure:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/PrivilegeStack.png)

The binary interfaces between User mode and Supervisor mode are named as Application Binary Interface (ABI), or another more famous one: `syscall`. 

Each time when a user mode app want to control the hardware(e.g., print a line on the screen), the following sequence will take place:

1. The app uses the `ecall` instruction to trigger a `trap`, which will cause the CPU to elevate current priviledge level and jump to the `trap handler` function set in the `stvec` register. 

```rust
global_asm!(include_str!("trap.S"));

pub fn init(){
    extern "C" {fn __alltraps();}
    unsafe {
        stvec::write(__alltraps as usize, TrapMode::Direct); // set the entry for trap handler
    }
}
```

2. The `trap handler` in the OS will first store the context of the app, then handles the trap according to its parameters. We implemeted this in `./os/src/trap/trap.S` 

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

3. In `trap handler` we handle the traps according to their type, or just terminate it and run next app if we think it's doing something bad.

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

4. After the desired operations have been executed, the OS will recover the context of the user mode app. Then the OS uses `sret` instruction to make the CPU reduce the priviledge level to user mode and jump back to the next line of the `ecall` in step 1.

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

The interaction between mode S and mode M is similar to above one, so in general the privilege level switching can be illustrated like this:

![](https://rcore-os.github.io/rCore-Tutorial-Book-v3/_images/EnvironmentCallFlow.png)

Also, the OS has the power to terminate a user mode program when necessary (e.g. the app try to use some priviledge instruction like `sret`).

>The associated code is placed in the `./os/trap` directory.

## 0x01 Batch Processing System

With the help of priviledges we can safely implement a batch processing system. The basic idea is straightfoward: each time we load a binary to address 0x80400000 then jump there to execute. Specifically, for each application we take the following operations:

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

3. Use `__restore` to fire up a application and set the kernel stack at the same time.

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

4. After an application finished, we move to next app.

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

## 0x02 Basic Security Check

Currently we only provide two syscalls for the user mode applications. As the `sys_write` may write to some important addresses, we should add some basic security checks on it. Here we only check whether the target interval covers addresses other than the user stack space and the app's storage space.

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

## References

https://rcore-os.github.io/rCore-Tutorial-Book-v3/chapter2/index.html

>All of the figures credit to rCore-Tutorial-Book-v3