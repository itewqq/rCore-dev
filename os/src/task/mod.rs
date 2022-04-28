mod context;
mod manager;
mod pid;
mod processor;
mod scheduler;
mod switch;
mod task;

use crate::fs::{open_file, OpenFlags};
use crate::mm::{MapPermission, VirtAddr};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::*;
pub use manager::{add_task, fetch_task};
pub use processor::{
    current_pid, current_task, current_trap_cx, current_user_token, run_tasks, schedule,
    take_current_task,
};
use scheduler::StrideScheduler;
use switch::__switch;
use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;

lazy_static! {
    pub static ref INITPROC: Arc<TaskControlBlock> = Arc::new({
        let inode = open_file("initproc", OpenFlags::RDONLY).unwrap();
        let v = inode.read_all();
        TaskControlBlock::new(v.as_slice())
    });
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

pub fn add_initproc() {
    add_task(INITPROC.clone());
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

pub fn set_current_prio(prio: usize) {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .set_prio(prio);
}

pub fn current_memory_set_mmap(
    start_va: VirtAddr,
    end_va: VirtAddr,
    permission: MapPermission,
) -> Result<(), &'static str> {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    inner
        .memory_set
        .insert_framed_area(start_va, end_va, permission)
}

pub fn current_memory_set_munmap(start_va: VirtAddr, end_va: VirtAddr) -> isize {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    inner.memory_set.remove_mapped_frames(start_va, end_va)
}

#[allow(unused)]
pub fn heap_test() {
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
