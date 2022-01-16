use riscv::register::{mtvec::TrapMode, scause::{self, Exception, Trap}, stval, stvec, sie};
use crate::syscall::syscall;
use crate::task::{
    exit_current_and_run_next,
    suspend_current_and_run_next,
};

mod context;

pub use self::context::TrapContext;
global_asm!(include_str!("trap.S"));

pub fn init(){
    extern "C" {fn __alltraps();}
    unsafe {
        stvec::write(__alltraps as usize, TrapMode::Direct); // set the entry for trap handler
    }
}

#[no_mangle]
pub fn trap_handler(ctx: &mut TrapContext) -> &mut TrapContext {
    let scause = scause::read();
    let stval = stval::read();
    // println!("{:?}", scause.cause());
    match scause.cause() {
        Trap::Exception(Exception::UserEnvCall) => {
            ctx.sepc += 4; // add bytecode length to point to ecall's next instruction
            ctx.x[10] = syscall(ctx.x[17], [ctx.x[10], ctx.x[11], ctx.x[12]]) as usize; // a0-a2
        }
        // handle other Exception
        Trap::Exception(Exception::StoreFault) |
        Trap::Exception(Exception::StorePageFault) => {
            println!("[kernel] PageFault in application, core dumped.");
            run_next_app();
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            println!("[kernel] IllegalInstruction in application, core dumped.");
            run_next_app();
        }
        _ => {
            println!("Unsupported trap {:?}, stval = {:#x}!", scause.cause(), stval);
            run_next_app();
        }
    }
    ctx
}