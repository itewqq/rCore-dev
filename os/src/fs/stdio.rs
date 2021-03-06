use super::{File, Stat};
use crate::mm::UserBuffer;
use crate::sbi::console_getchar;
use crate::task::{current_add_signal, suspend_current_and_run_next, SignalFlags};

pub struct Stdin;

pub struct Stdout;

impl File for Stdin {
    fn readable(&self) -> bool {
        true
    }

    fn writable(&self) -> bool {
        false
    }

    fn read(&self, mut user_buf: UserBuffer) -> usize {
        assert_eq!(user_buf.len(), 1);
        // busy loop, give up the CPU if there is no input
        let mut c: usize;
        loop {
            c = console_getchar();
            if c == 0 {
                suspend_current_and_run_next();
                continue;
            } else if c == 3 {
                current_add_signal(SignalFlags::SIGINT);
                break;
            } else {
                break;
            }
        }
        let ch = c as u8;
        unsafe {
            user_buf.buffers[0].as_mut_ptr().write_volatile(ch);
        }
        1
    }

    fn write(&self, _user_buf: UserBuffer) -> usize {
        error!("Cannot write to stdin!");
        0
    }

    fn fstat(&self) -> Stat {
        Stat::new()
    }
}

impl File for Stdout {
    fn readable(&self) -> bool {
        false
    }

    fn writable(&self) -> bool {
        true
    }

    fn read(&self, _user_buf: UserBuffer) -> usize {
        error!("Cannot read from stdout!");
        0
    }

    fn write(&self, user_buf: UserBuffer) -> usize {
        for buffer in user_buf.buffers.iter() {
            print!("{}", core::str::from_utf8(*buffer).unwrap());
        }
        user_buf.len()
    }

    fn fstat(&self) -> Stat {
        Stat::new()
    }
}
