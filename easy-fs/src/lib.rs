#![no_std]

extern crate alloc;

mod bitmap;
mod block_dev;
mod block_cache;

pub const BLOCK_SZ: usize = 512;
pub use block_dev::BlockDevice;
pub use block_cache::{get_block_cache, block_cache_sync_all};