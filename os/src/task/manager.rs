use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use lazy_static::*;

use super::scheduler::BIG_STRIDE;
use super::StrideScheduler;
use super::TaskControlBlock;
use crate::sync::UPSafeCell;

type Scheduler = StrideScheduler;

pub struct TaskManager {
    ready_queue: BTreeMap<usize, Arc<TaskControlBlock>>,
    scheduler: Box<Scheduler>,
}

impl TaskManager {
    pub fn new() -> Self {
        let stride_scheduler: Scheduler = Scheduler::new();
        Self {
            ready_queue: BTreeMap::new(),
            scheduler: Box::new(stride_scheduler),
        }
    }

    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        // update pass for stride scheduler
        let mut task_inner = task.inner_exclusive_access();
        task_inner.pass += BIG_STRIDE / task_inner.priority;
        drop(task_inner);
        self.scheduler
            .insert_task(task.getpid(), task.clone().inner_exclusive_access().pass);
        self.ready_queue.insert(task.getpid(), task);
    }

    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        let next_pid = loop {
            if let Some(next_pid) = self.scheduler.find_next_task() {
                // TODO how about wait state
                if self.ready_queue.contains_key(&next_pid) {
                    break next_pid;
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
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}

pub fn pid2task(pid: usize) -> Option<Arc<TaskControlBlock>> {
    let map = &TASK_MANAGER.exclusive_access().ready_queue;
    map.get(&pid).map(Arc::clone)
}
