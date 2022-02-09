use core::cmp::{Ord, Ordering};
use alloc::collections::BinaryHeap;

pub const BIG_STRIDE: usize = 1_000;

#[derive(Copy, Clone, Eq)]
pub struct Stride {
    id: usize,
    pass: usize,
}

impl Stride {
    pub fn new(id: usize, pass: usize) -> Self {
        Self { id, pass, }
    }

    pub fn zeros() -> Self {
        Self { id: 0, pass: 0, }
    }
}

impl Ord for Stride {
    fn cmp(&self, other: &Self) -> Ordering {
        self.pass.cmp(&other.pass).reverse()
    }
}

impl PartialOrd for Stride {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Stride {
    fn eq(&self, other: &Self) -> bool {
        self.pass == other.pass
    }
}

pub struct StrideScheduler {
    queue: BinaryHeap<Stride>,
}

impl StrideScheduler {
    pub fn new() -> Self {
        Self {queue: BinaryHeap::new()}
    }

    pub fn create_task(&mut self, id: usize) {
        self.queue.push(Stride::new(id, 0));
    }

    pub fn insert_task(&mut self, id: usize, pass: usize){
        self.queue.push(Stride::new(id, pass));
    }

    pub fn find_next_task(&mut self) -> Option<usize> {
        let next = self.queue.pop();
        if let Some(node) = next {
            Some(node.id)
        } else {
            None
        }
    }
}

#[allow(unused)]
pub fn scheduler_order_test() {
    let mut strid_sched = StrideScheduler::new();
    strid_sched.insert_task(0, 100);
    strid_sched.insert_task(1, 200);
    strid_sched.insert_task(2, 300);
    assert_eq!(strid_sched.find_next_task(), Some(0));
    assert_eq!(strid_sched.find_next_task(), Some(1));
    assert_eq!(strid_sched.find_next_task(), Some(2));
    println!("Stride test passed!");
}