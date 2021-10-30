use riscv::register::sstatus::{Sstatus, self, SPP};

pub struct TrapContext {
    pub x: [usize; 32],
    pub sstatus: Sstatus,
    pub spec: usize,
}