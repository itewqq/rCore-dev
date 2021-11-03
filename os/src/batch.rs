use core::cell::RefCell;
use lazy_static::*;
use crate::trap::TrapContext;

// Kernel stack

pub const USER_STACK_SIZE: usize = 4096;
const KERNEL_STACK_SIZE: usize = 4096 * 2;

#[repr(align(4096))]
struct KernelStack {
    data: [u8; KERNEL_STACK_SIZE],
}

#[repr(align(4096))]
pub struct UserStack {
    data: [u8; USER_STACK_SIZE],
}

impl KernelStack {
    fn get_sp(&self) -> usize {self.data.as_ptr() as usize + KERNEL_STACK_SIZE}

    pub fn push_context(&self, ctx: TrapContext) -> &'static mut TrapContext {
        let ctx_ptr = (self.get_sp() - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
        unsafe { *ctx_ptr = ctx; } // copy ctx to stack
        unsafe { ctx_ptr.as_mut().unwrap() }
    }
}

impl UserStack {
    pub fn get_sp(&self) -> usize {self.data.as_ptr() as usize + USER_STACK_SIZE}
}

static KERNEL_STACK: KernelStack = KernelStack { data: [0; KERNEL_STACK_SIZE] };
pub static USER_STACK: UserStack = UserStack { data: [0; USER_STACK_SIZE] };


// AppManager

pub const MAX_APP_NUM: usize = 16;
pub const APP_BASE_ADDRESS: usize = 0x80400000;
pub const APP_SIZE_LIMIT: usize = 0x20000;

pub struct AppManager {
    pub inner: RefCell<AppManagerInner>,
}

pub struct AppManagerInner {
    num_app: usize,
    current_app: usize,
    app_start_addrs: [usize; MAX_APP_NUM+1],
}

unsafe impl Sync for AppManager{}

impl AppManagerInner{

    pub fn print_app_addrs(&self) {
        println!("Number of app is {}", self.num_app);
        for i in 0..self.num_app {
            println!("[kernel] app {} starts at {:#x}, ends at {:#x}", 
                    i, self.app_start_addrs[i], self.app_start_addrs[i+1]);
        }
    }

    pub fn get_current_app(&self) -> usize {self.current_app}

    pub fn get_current_app_size(&self) -> usize {self.app_start_addrs[self.current_app+1]-self.app_start_addrs[self.current_app]}

    pub fn move_to_next_app(&mut self) {self.current_app+=1;}

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
}

// lazy_static will be initialized when the object is first used
lazy_static! {
    pub static ref APP_MANAGER: AppManager = AppManager {
        inner: RefCell::new({
            extern "C" { fn _num_app(); } // label in link_app.S
            let num_app_ptr = _num_app as usize as *const usize;
            let num_app = unsafe { num_app_ptr.read_volatile() };
            let mut app_start_addrs: [usize; MAX_APP_NUM + 1] = [0; MAX_APP_NUM + 1];
            let app_start_raw: &[usize] = unsafe {
                core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1)
            };
            app_start_addrs[..=num_app].copy_from_slice(app_start_raw);
            AppManagerInner {
                num_app,
                current_app: 0,
                app_start_addrs,
            }
        }),
    };
}

pub fn init() {
    print_app_info();
}

pub fn print_app_info() {
    APP_MANAGER.inner.borrow().print_app_addrs();
}

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