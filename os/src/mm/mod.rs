mod address;
mod heap_allocator;
mod page_table;
mod frame_allocator;
mod memory_set;

pub use address::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
pub use address::{StepByOne, VPNRange};
pub use frame_allocator::{frame_alloc, FrameTracker};
pub use memory_set::remap_test;
pub use memory_set::{MapPermission, MemorySet, KERNEL_SPACE, MapArea, MapType};
pub use page_table::{PageTableEntry, translated_byte_buffer};
pub use page_table::{PTEFlags, PageTable};

pub fn init() {
    heap_allocator::init_heap();
    frame_allocator::init_frame_allocator();
    KERNEL_SPACE.exclusive_access().activate();
}
