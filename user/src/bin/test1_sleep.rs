#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use core::fmt::Debug;

use user_lib::{get_time, yield_};

/// 正确输出：（无报错信息）
/// get_time OK! {...}
/// Test sleep OK!

#[no_mangle]
fn main() -> i32 {
    debug!("Got user space");
    let current_time = get_time();
    assert!(current_time > 0);
    println!("get_time OK! {}", current_time);
    let wait_for = current_time + 3000;
    while get_time() < wait_for {
        yield_();
    }
    println!("Test sleep OK!");
    0
}
