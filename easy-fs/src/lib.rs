#![no_std]

extern crate alloc;

mod bitmap;
mod block_cache;
mod block_dev;
mod efs;
mod layout;

pub const BLOCK_SZ: usize = 512;
use bitmap::Bitmap;
pub use block_cache::{block_cache_sync_all, get_block_cache};
pub use block_dev::BlockDevice;
pub use efs::EasyFileSystem;
use layout::*;
