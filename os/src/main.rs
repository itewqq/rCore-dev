#![no_std]
#![no_main]
#![allow(dead_code)]
#![feature(asm)]
#![feature(global_asm)]
#![feature(panic_info_message)]

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
    println!("[kernel] Hello, world!");
    trap::init();
    panic!("Unreachable in rust_main!");
}