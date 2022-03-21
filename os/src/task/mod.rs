mod context;
mod switch;
mod task;
pub mod scheduler;

use crate::loader::{get_num_app};
use crate::mm::{MapPermission, VirtPageNum, VirtAddr};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use crate::loader::get_app_data;
use alloc::boxed::Box;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
use task::{TaskControlBlock, TaskStatus};
use scheduler::{BIG_STRIDE, StrideScheduler};

pub use context::TaskContext;

pub struct TaskManager {
    num_app: usize,
    inner: UPSafeCell<TaskManagerInner>,
}

struct TaskManagerInner {
    tasks: Vec<TaskControlBlock>,
    current_task: usize,
    scheduler: Box<StrideScheduler>,
}

lazy_static! {
    pub static ref TASK_MANAGER: TaskManager = {
        println!("init TASK_MANAGER");
        let num_app = get_num_app();
        println!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        let mut stride_scheduler: StrideScheduler = StrideScheduler::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
            stride_scheduler.create_task(i);
        }
        TaskManager {
            num_app,
            inner: unsafe { UPSafeCell::new(TaskManagerInner {
                tasks,
                current_task: 0,
                scheduler: Box::new(stride_scheduler),
            })},
        }
    };
}

impl TaskManager {
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next = inner.scheduler.find_next_task().unwrap();// let it panic or not
        let task0 = &mut inner.tasks[next];
        task0.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &task0.task_cx as *const TaskContext;
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(
                &mut _unused as *mut TaskContext,
                next_task_cx_ptr,
            );
        }
        panic!("unreachable in run_first_task!");
    }

    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Ready;
    }

    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Exited;
    }

    fn find_next_task(&self) -> Option<usize> {
        let mut inner = self.inner.exclusive_access();
        loop {
            let next = inner.scheduler.find_next_task();
            if let Some(id) = next {
                if inner.tasks[id].task_status == TaskStatus::Ready {
                    return next;
                }else {
                    continue; // no ready so removed? 
                }
            }else {
                return None;
            }
        }
    }

    fn run_next_task(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].pass += BIG_STRIDE / inner.tasks[current].priority;
        let current_pass = inner.tasks[current].pass;
        inner.scheduler.insert_task(current, current_pass);
        drop(inner);
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(
                    current_task_cx_ptr,
                    next_task_cx_ptr,
                );
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }

    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    fn get_current_trap_cx(&self) -> &'static mut TrapContext {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_trap_cx()
    }

    fn get_current_id(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.current_task
    }

    fn current_memory_set_mmap(&self, start_va: VirtAddr, end_va: VirtAddr, permission: MapPermission) -> Result<(), &'static str > {
        let mut inner = self.inner.exclusive_access();
        let current_task = inner.current_task;
        inner.tasks[current_task].memory_set.insert_framed_area(start_va, end_va, permission)
    }

    fn current_memory_set_munmap(&self, start_va: VirtAddr, end_va: VirtAddr) -> isize {
        let mut inner = self.inner.exclusive_access();
        let current_task = inner.current_task;
        inner.tasks[current_task].memory_set.remove_mapped_frames(start_va, end_va)
    }

    fn set_current_prio(&self, prio: usize) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].priority = prio;
    }
}

pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

pub fn set_current_prio(prio: usize) {
    let prio = if prio < BIG_STRIDE {prio} else {BIG_STRIDE};
    TASK_MANAGER.set_current_prio(prio);
}

pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

pub fn current_id() -> usize {
    TASK_MANAGER.get_current_id()
}

pub fn current_memory_set_mmap(start_va: VirtAddr, end_va: VirtAddr, permission: MapPermission) -> Result<(), &'static str > {
    TASK_MANAGER.current_memory_set_mmap(start_va, end_va, permission)
}

pub fn current_memory_set_munmap(start_va: VirtAddr, end_va: VirtAddr) -> isize {
    TASK_MANAGER.current_memory_set_munmap(start_va, end_va)
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