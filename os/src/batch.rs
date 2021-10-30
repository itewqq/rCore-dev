use core::cell::RefCell;
use lazy_static::*;

// Kernel stack

const USER_STACK_SIZE: usize = 4096 * 2;
const KERNEL_STACK_SIZE: usize = 4096 * 2;

#[repr(align(4096))]
struct KernelStack {
    data: [u8; KERNEL_STACK_SIZE],
}

#[repr(align(4096))]
struct UserStack {
    data: [u8; USER_STACK_SIZE],
}

impl KernelStack {
    fn get_sp(&self) -> usize {self.data.as_ptr() as usize + KERNEL_STACK_SIZE}
}

impl UserStack {
    fn get_sp(&self) -> usize {self.data.as_ptr() as usize + USER_STACK_SIZE}
}

static KERNEL_STACK: KernelStack = KernelStack { data: [0; KERNEL_STACK_SIZE] };
static USER_STACK: UserStack = UserStack { data: [0; USER_STACK_SIZE] };


// AppManager

const MAX_APP_NUM: usize = 16;
const APP_BASE_ADDRESS: usize = 0x80400000;
const APP_SIZE_LIMIT: usize = 0x20000;

struct AppManager {
    inner: RefCell<AppManagerInner>,
}

struct AppManagerInner {
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

    pub fn move_to_next_app(&mut self) {self.current_app+=1;}

    unsafe fn load_app(&self, app_id: usize) {
        if app_id >= self.num_app {
            panic!("All applications completed!");
        }
        println!("[kernel] Loading app{} ...", app_id);
        llvm_asm!("fence.i" :::: "volatile"); // clear i-cache
        for addr in APP_BASE_ADDRESS..APP_BASE_ADDRESS+APP_SIZE_LIMIT {
            (addr as *mut u8).write_volatile(0);
        }
        let src = core::slice::from_raw_parts(
            (APP_BASE_ADDRESS+self.app_start_addrs[app_id]) as *const u8, 
            self.app_start_addrs[app_id+1]-self.app_start_addrs[app_id]);
        let dst = core::slice::from_raw_parts_mut(APP_BASE_ADDRESS as *mut u8,
            self.app_start_addrs[app_id+1]-self.app_start_addrs[app_id]);
        dst.copy_from_slice(src);
    }
}

lazy_static! {
    static ref APP_MANAGER: AppManager = AppManager {
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

