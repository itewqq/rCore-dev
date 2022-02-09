pub const USER_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_HEAP_SIZE: usize = 0x30_000;
pub const MAX_APP_NUM: usize = 8;
pub const APP_BASE_ADDRESS: usize = 0x80400000;
pub const APP_SIZE_LIMIT: usize = 0x20000;

// #[cfg(feature = "board_qemu")]
pub const CLOCK_FREQ: usize = 12500000;