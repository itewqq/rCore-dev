mod address;
mod heap_allocator;
mod page_table;
mod frame_allocator;

pub fn init() {
    heap_allocator::init_heap();
}
