const FD_STDOUT: usize = 1;

pub fn sys_write(fd: usize, buffer: *const u8, len: usize) -> isize {
    match fd {
        FD_STDOUT => {
            let raw_bytes = unsafe { core::slice::from_raw_parts(buffer, len) };
            let str = core::str::from_utf8(raw_bytes).unwrap();
            print!("{}", str);
            len as isize
        },
        _ => {
            panic!("Unsupported fd type: {}", fd)
        }
    }
}