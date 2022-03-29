use alloc::sync::Arc;
use alloc::collections::{
    VecDeque,
    BTreeMap,
};
use alloc::boxed::Box;
use lazy_static::*;

use crate::sync::UPSafeCell;
use super::StrideScheduler;
use super::TaskControlBlock;
use super::pid::PidHandle;

type Scheduler = StrideScheduler;

pub struct TaskManager {
    ready_queue: BTreeMap<usize, Arc<TaskControlBlock>>,
    scheduler: Box<Scheduler>,
}

impl TaskManager {
    pub fn new() -> Self {
        let mut stride_scheduler: Scheduler = Scheduler::new();
        Self {
            ready_queue: BTreeMap::new(),
            scheduler: Box::new(stride_scheduler),
        }
    }

    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.scheduler.insert_task(task.getpid(), 0);
        self.ready_queue.insert(task.getpid(), task);
    }

    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        let next_pid = loop {
            if let Some(next_pid) = self.scheduler.find_next_task() {
                // TODO how about wait state
                if self.ready_queue.contains_key(&next_pid){
                    break next_pid
                }
            } else {
                return None;
            }
            
        };
        let (_, task) = self.ready_queue.remove_entry(&next_pid).unwrap();
        Some(task)
    }
}

lazy_static! {
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> = unsafe {
        UPSafeCell::new(TaskManager::new())
    };
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}