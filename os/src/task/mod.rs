mod context;
mod switch;
mod task;
mod pid;
mod manager;
mod scheduler;
mod processor;

use crate::loader::{get_num_app};
use crate::mm::{MapPermission, VirtAddr};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use crate::loader::get_app_data;
use crate::loader::get_app_data_by_name;

use alloc::sync::Arc;
use alloc::boxed::Box;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
use task::{TaskControlBlock, TaskStatus};
use scheduler::{BIG_STRIDE, StrideScheduler};
pub use manager::{add_task, fetch_task};
pub use processor::{take_current_task, current_task, current_user_token, current_trap_cx, schedule};

pub use context::TaskContext;

lazy_static! {
    pub static ref INITPROC: Arc<TaskControlBlock> = Arc::new(
        TaskControlBlock::new(get_app_data_by_name("initproc").unwrap())
    );
}

pub struct TaskManager {
    num_app: usize,
    inner: UPSafeCell<TaskManagerInner>,
}

struct TaskManagerInner {
    tasks: Vec<TaskControlBlock>,
    current_task: usize,
    scheduler: Box<StrideScheduler>,
}

pub fn suspend_current_and_run_next() {
    let task = take_current_task().unwrap();

    // ---- access current TCB exclusively
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // Change status to Ready
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    // ---- stop exclusively accessing current PCB

    // push back to ready queue.
    add_task(task);
    // jump to scheduling cycle
    schedule(task_cx_ptr);
}

pub fn exit_current_and_run_next(exit_code: i32) {
    // take from processor
    let task = take_current_task().unwrap();
    // access current TCB exclusively
    let mut inner = task.inner_exclusive_access();
    // change status to Zombie
    inner.task_status = TaskStatus::Zombie;
    // record exit code
    inner.exit_code = exit_code;
    // initproc collects children
    {
        let mut initproc_inner = INITPROC.inner_exclusive_access();
        for child in inner.children.iter() {
            child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
            initproc_inner.children.push(child.clone());
        }
    }
    inner.children.clear();
    // dealloc memory in user space, 
    // but the page table in phys memory still here and will be recycled by parent with sys_waitpid
    inner.memory_set.recycle_data_pages();
    drop(inner);
    // drop task, so there is only one ref to it in it's parent
    drop(task);
    // No task context
    let mut _unused = TaskContext::zero_init();
    schedule(&mut _unused as *mut _);
}

#[allow(unused)]
pub fn heap_test(){
    use alloc::collections::BinaryHeap;
    let mut heap = BinaryHeap::new();
    heap.push(1);
    heap.push(5);
    heap.push(2);
    assert_eq!(heap.pop(), Some(5));
    assert_eq!(heap.pop(), Some(2));
    assert_eq!(heap.pop(), Some(1));
    assert_eq!(heap.pop(), None);
    debug!("heap test success!");
}