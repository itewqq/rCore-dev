use crate::config::{KERNEL_STACK_SIZE, PAGE_SIZE, TRAMPOLINE, TRAP_CONTEXT_BASE, USER_STACK_SIZE};
use crate::mm::{MapPermission, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use crate::task::process::ProcessControlBlock;
use alloc::{sync::Weak, vec::Vec};
use lazy_static::*;

pub struct RecycleAllocator {
    current: usize,
    recycled: Vec<usize>,
}

impl RecycleAllocator {
    pub fn new() -> Self {
        RecycleAllocator {
            current: 0,
            recycled: Vec::new(),
        }
    }

    pub fn alloc(&mut self) -> usize {
        if let Some(pid) = self.recycled.pop() {
            pid
        } else {
            self.current += 1;
            self.current - 1
        }
    }

    pub fn dealloc(&mut self, pid: usize) {
        assert!(pid < self.current);
        assert!(
            !self.recycled.iter().any(|ppid| *ppid == pid),
            "pid {} has been deallocated!",
            pid
        );
        self.recycled.push(pid);
    }
}

lazy_static! {
    static ref PID_ALLOCATOR: UPSafeCell<RecycleAllocator> =
        unsafe { UPSafeCell::new(RecycleAllocator::new()) };
    static ref KSTACK_ALLOCATOR: UPSafeCell<RecycleAllocator> =
        unsafe { UPSafeCell::new(RecycleAllocator::new()) };
}

pub const IDLE_PID: usize = 0;

pub struct PidHandle(pub usize);

impl Drop for PidHandle {
    fn drop(&mut self) {
        PID_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}

pub fn pid_alloc() -> PidHandle {
    PidHandle(PID_ALLOCATOR.exclusive_access().alloc())
}

pub fn kernel_stack_position(pid: usize) -> (usize, usize) {
    let top = TRAMPOLINE - pid * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom = top - KERNEL_STACK_SIZE;
    (bottom, top)
}

pub struct KernelStack {
    kstack_id: usize,
}

impl KernelStack {
    pub fn new() -> Self {
        let kstack_id = KSTACK_ALLOCATOR.exclusive_access().alloc();
        let (kstack_bottom, kstack_top) = kernel_stack_position(kstack_id);
        if let Err(e) = KERNEL_SPACE.exclusive_access().insert_framed_area(
            kstack_bottom.into(),
            kstack_top.into(),
            MapPermission::R | MapPermission::W,
        ) {
            error!("Cannot allocate kernel stack for {}, {}", kstack_id, e);
            return Self {
                kstack_id: usize::MAX,
            };
        }
        Self { kstack_id }
    }

    pub fn get_top(&self) -> usize {
        let (_, top) = kernel_stack_position(self.kstack_id);
        top
    }

    pub fn put_on_top<T>(&self, value: T) -> *mut T
    where
        T: Sized,
    {
        let top = self.get_top();
        let ptr_mut = (top - core::mem::size_of::<T>()) as *mut T;
        unsafe { *ptr_mut = value };
        ptr_mut
    }
}

impl Drop for KernelStack {
    fn drop(&mut self) {
        let (kernel_stack_bottom, _) = kernel_stack_position(self.kstack_id);
        let kernel_stack_bottom_va: VirtAddr = kernel_stack_bottom.into();
        KERNEL_SPACE
            .exclusive_access()
            .remove_area_with_start_vpn(kernel_stack_bottom_va.into());
    }
}

fn trap_cx_bottom_from_tid(tid: usize) -> usize {
    TRAP_CONTEXT_BASE - tid * PAGE_SIZE
}

fn ustack_bottom_from_tid(ustack_base: usize, tid: usize) -> usize {
    ustack_base + tid * (PAGE_SIZE + USER_STACK_SIZE)
}

pub struct TaskUserRes {
    pub tid: usize,
    pub ustack_base: usize,
    pub process: Weak<ProcessControlBlock>,
}

