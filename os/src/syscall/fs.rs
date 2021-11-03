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