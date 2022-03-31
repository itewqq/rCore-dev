#![no_std]
#![no_main]
#![allow(dead_code)]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

use core::arch::global_asm;
extern crate alloc;

#[macro_use]
extern crate bitflags;

#[macro_use]
mod console;
mod lang_items;
mod sbi;
mod syscall;
mod trap;
mod task;
mod sync;
mod timer;
mod config;
mod loader;
mod mm;

global_asm!(include_str!("entry.asm"));
global_asm!(include_str!("link_app.S"));

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
    kprintln!("paging enabled");
    task::add_initproc();
    trap::init();
    kprintln!("all traps enabled");
    trap::enable_timer_interrupt();
    timer::set_next_trigger();
    kprintln!("timer trigger enabled");
    loader::list_apps();
    task::run_tasks();
    panic!("Unreachable in rust_main!");
}