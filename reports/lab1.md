# Lab1: A Trilobite OS

## 0x00 Get rid of standard library dependencies

This is the first challenge for any software developer start moving to system development: You can not rely on ANY standard libraries (glibc, uclibc, klibc or any other implementations), since the OS itself is the one responsible for providing these libs. Let's try to get rid of them.

In Rust and C/C++ (and almost all programming languages), before running into ```main()```, the execution environment will do some initialization work, where the ```std``` library and other standard libraries (GNU libc) may be used. Thus we have to tell ```Cargo``` there is no ```main``` and ```std``` in our target.

```rust
// os/src/main.rs
#![no_std]
#![no_main]
```

And we need to explicitly write a ```_start()``` function, which is the entry ```Cargo``` is looking for.

```rust
// os/src/main.rs
#[no_mangle]
extern "C" fn _start() {
    // Nothing here now
}
```

Besides, ```Cargo``` requires us to provide ```panic_handler``` or it will not compile. Usually the ```std``` will take care of that but now we have to manually add a ```panic_handler```.

```rust
// os/src/lang_items.rs
use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // Nothing here now
}
```

>Note that the ```rust-core``` can be used (and very useful) on bare metal.

Next, we need to make it possible to run our program directly on CPU without any OS support.

## 0x01 Make the CPU run it

For an odinary program, running it is easy: All you have to do is type it's name in a shell and hit Enter, or double-click the exe file in Windows. That ease is benefiting from the OS. As we are creating an OS, things can get a little more complicated. Let's first think about what will happen when the CPU starts to working.

When the CPU (riscv64 emulated by QEMU in our case) is powered on, the other general registers of the CPU are cleared to zero, and the PC register will point to the ```0x1000``` location. This ```0x1000``` location is the first instruction executed after the CPU is powered up (a small piece of boot code solidified in the hardware), and it will quickly jump to ```0x80000000```, which is the first instruction of the BootLoader program - ```RustSBI```. After the basic hardware initialization, ```RustSBI``` will jump to the operating system binary code memory location ```0x80200000``` (for QEMU) and execute the first instruction of the operating system. Then our written operating system starts to work.

>About the SBI: SBI is an underlying specification for RISC-V. The relationship between the operating system kernel and ```RustSBI```, which implements the SBI specification, is somewhat like the relationship between an application and the operating system kernel, with the latter providing certain services to the former. However, SBI provides few services and can help the OS kernel to perform limited functions, but these functions are very low-level and important, such as shutting down the computer, displaying strings, and so on. If ```RustSBI``` provides services, then the OS kernel can call them directly.

So it's clear that we have to put our built OS at the ```0x80200000``` address (for QEMU). By default, ```Cargo``` adopts a usermode memory layout which is not we expected, for example we will not get a entry address at ```0x80200000``` in the generated binary. To address that we need a custom linker script to make every section's location right:

```ld
OUTPUT_ARCH(riscv)
ENTRY(_start)
BASE_ADDRESS = 0x80200000;

SECTIONS
{
    . = BASE_ADDRESS;
    skernel = .;

    stext = .;
    .text : {
        *(.text.entry)
        *(.text .text.*)
    }

    . = ALIGN(4K);
    etext = .;
    srodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }

    . = ALIGN(4K);
    erodata = .;
    sdata = .;
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }

    . = ALIGN(4K);
    edata = .;
    .bss : {
        *(.bss.stack)
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }

    . = ALIGN(4K);
    ebss = .;
    ekernel = .;

    /DISCARD/ : {
        *(.eh_frame)
    }
}
```

Then we force Cargo to use it in linking:

```toml
// os/.cargo/config
[build]
target = "riscv64gc-unknown-none-elf"

[target.riscv64gc-unknown-none-elf]
rustflags = [
    "-Clink-arg=-Tsrc/linker.ld", "-Cforce-frame-pointers=yes"
]
```

## 0x02 Allocate stack space properly

In order to make our program run properly, we also need a ```Stack```, which is used to store/load data quickly when we are short of registers, such as return address of current function, stack frame pointer, local variables, etc. Unlike the linker script used before, the compiler cannot help us to arrange the stack, so we have to use a piece of assembly code to allocate stack space at the beginning of our OS execution.

```assembly
# os/src/entry.asm
    .section .text.entry
    .globl _start
_start:
    la sp, boot_stack_top
    call rust_main

    .section .bss.stack
    .globl boot_stack
boot_stack:
    .space 4096 * 16
    .globl boot_stack_top
boot_stack_top:
```

Note that we move the ```_start``` symbol here. We first set the ```sp``` register to a $64KiB$ space, then goto label ```rust_main```. We modify the ```main.rs``` as follows:

```rust
// os/src/main.rs
#![no_std]
#![no_main]
#![feature(llvm_asm)]
#![feature(global_asm)]
#![feature(panic_info_message)]

mod lang_items;

global_asm!(include_str!("entry.asm"));

#[no_mangle]
pub fn rust_main() -> ! {
    // Nothing here now
}
```

Also don't forget to clear the ```.bss``` segment, which is considered to be a standard behavior in modern operating systems.

```rust
// os/src/main.rs
fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    (sbss as usize..ebss as usize).for_each(|a| {
        unsafe { (a as *mut u8).write_volatile(0) }
    });
}
```

## 0x03 Add functions to our OS

At this stage we will add some basic functions to our OS, only to make it able to successfully run on bare metal. In order to achieve this we need to have the help of SBI. In layman's terms, SBI is like an OS for OS, which provides many useful low-level functions. 

>If you are confused about SBI, go back to the comment in Section 0x01

Here we will, as said before, only use a few basic sys_calls of SBI, which are read/write a character to console and shutdown the machine.

```rust
// os/src/sbi.rs
#![allow(unused)]
#![allow(deprecated)]

const SBI_SET_TIMER: usize = 0;
const SBI_CONSOLE_PUTCHAR: usize = 1;
const SBI_CONSOLE_GETCHAR: usize = 2;
const SBI_CLEAR_IPI: usize = 3;
const SBI_SEND_IPI: usize = 4;
const SBI_REMOTE_FENCE_I: usize = 5;
const SBI_REMOTE_SFENCE_VMA: usize = 6;
const SBI_REMOTE_SFENCE_VMA_ASID: usize = 7;
const SBI_SHUTDOWN: usize = 8;

fn sbi_call(which: usize, arg0: usize, arg1: usize, arg2: usize) -> usize {
    let mut ret;
    unsafe {
        llvm_asm!("ecall"
            : "={x10}" (ret)
            : "{x10}" (arg0), "{x11}" (arg1), "{x12}" (arg2), "{x17}" (which)
            : "memory"
            : "volatile"
        );
    }
    ret
}

pub fn console_putchar(c: usize) {
    sbi_call(SBI_CONSOLE_PUTCHAR, c, 0, 0);
}

pub fn console_getchar() -> usize {
    sbi_call(SBI_CONSOLE_GETCHAR, 0, 0, 0)
}

pub fn shutdown() -> ! {
    sbi_call(SBI_SHUTDOWN, 0, 0, 0);
    panic!("It should shutdown!");
}
```

Based on these SBI sys_calls, we can implement some basic console-interactive functoins:

```rust
// os/src/console.rs
#![allow(dead_code)]

use crate::sbi::console_putchar;
use core::fmt::{self, Write};

struct Stdout;

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            console_putchar(c as usize);
        }
        Ok(())
    }
}

pub fn print(args: fmt::Arguments) {
    Stdout.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! print {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(format_args!($fmt $(, $($arg)+)?));
    }
}

#[macro_export]
macro_rules! println {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(format_args!(concat!($fmt, "\n") $(, $($arg)+)?));
    }
}

#[macro_export]
macro_rules! error {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(format_args!(concat!("\x1b[0;31m", $fmt, "\x1b[0m\n") $(, $($arg)+)?));
    }
}

#[macro_export]
macro_rules! info {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(format_args!(concat!("\x1b[0;34m", $fmt, "\x1b[0m\n") $(, $($arg)+)?));
    }
}

#[macro_export]
macro_rules! debug {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(format_args!(concat!("\x1b[0;32m", $fmt, "\x1b[0m\n") $(, $($arg)+)?));
    }
}
```

## 0x04 Final result

Finally we can build and run our super naive OS on a bare metal! Let's add some test code:

```rust
// os/src/main.rs
pub fn rust_main() -> ! {
    extern "C" {
        fn stext();
        fn etext();
        fn srodata();
        fn erodata();
        fn sdata();
        fn edata();
        fn sbss();
        fn ebss();
        fn boot_stack();
        fn boot_stack_top();
    }
    clear_bss();
    info!("Hello, info!");
    debug!("Hello, world!");
    info!(".text [{:#x}, {:#x})", stext as usize, etext as usize);
    info!(".rodata [{:#x}, {:#x})", srodata as usize, erodata as usize);
    info!(".data [{:#x}, {:#x})", sdata as usize, edata as usize);
    info!(
        "boot_stack [{:#x}, {:#x})",
        boot_stack as usize, boot_stack_top as usize
    );
    info!(".bss [{:#x}, {:#x})", sbss as usize, ebss as usize);
    panic!("Shutdown machine!");
}
```

In the above code we add some basic output sentences to test our OS. At the end, we shutdown the machine through a ```panic!```, which now should be:

```rust
// os/src/lang_items.rs
use core::panic::PanicInfo;
use crate::sbi::shutdown;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(location) = info.location() {
        error!("Panicked at {}:{} {}", location.file(), location.line(), info.message().unwrap());
    } else {
        error!("Panicked: {}", info.message().unwrap());
    }
    shutdown()
}
```

Let's test our OS in QEMU:

```rust
$ cargo build --release
$ rust-objcopy --binary-architecture=riscv64 target/riscv64gc-unknown-none-elf/release/os \
    --strip-all -O binary target/riscv64gc-unknown-none-elf/release/os.bin
$ qemu-system-riscv64 \
    -machine virt \
    -nographic \
    -bios ../bootloader/rustsbi-qemu.bin \
    -device loader,file=target/riscv64gc-unknown-none-elf/release/os.bin,addr=0x80200000

    [rustsbi] Version 0.1.0
    .______       __    __      _______.___________.  _______..______   __
    |   _  \     |  |  |  |    /       |           | /       ||   _  \ |  |
    |  |_)  |    |  |  |  |   |   (----`---|  |----`|   (----`|  |_)  ||  |
    |      /     |  |  |  |    \   \       |  |      \   \    |   _  < |  |
    |  |\  \----.|  `--'  |.----)   |      |  |  .----)   |   |  |_)  ||  |
    | _| `._____| \______/ |_______/       |__|  |_______/    |______/ |__|

    [rustsbi] Platform: QEMU
    [rustsbi] misa: RV64ACDFIMSU
    [rustsbi] mideleg: 0x222
    [rustsbi] medeleg: 0xb1ab
    [rustsbi] Kernel entry: 0x80200000
    Hello, world!
    Panicked at src/main.rs:95 It should shutdown!
```

Cheers to that! 

However, this is only a small program that can run on a bare metal, far from being an operating system (that' s why it is called the trilobite system, for little work can be done) . We still have a lot of cool code to write, lol