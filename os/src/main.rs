#![no_std]
#![no_main]
#![allow(dead_code)]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

use core::arch::global_asm;
extern crate alloc;

#[macro_use]
extern crate bitflags;

#[path = "boards/qemu.rs"]
mod board;

#[macro_use]
mod console;
mod config;
mod drivers;
mod fs;
mod lang_items;
mod mm;
mod sbi;
mod sync;
mod syscall;
mod task;
mod timer;
mod trap;

global_asm!(include_str!("entry.asm"));

fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    (sbss as usize..ebss as usize).for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}

#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();
    mm::init();
    info!("paging enabled...");
    task::add_initproc();
    trap::init();
    info!("all traps enabled...");
    trap::enable_timer_interrupt();
    timer::set_next_trigger();
    info!("timer trigger enabled...");
    kprintln!("Welcome to rCore OS!");
    task::run_tasks();
    panic!("Unreachable in rust_main!");
}
