use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};

use crate::{mm::MemorySet, sync::UPSafeCell};
use crate::fs::File;
use crate::task::{SignalFlags, SignalActions, TaskControlBlock};

use super::id::{PidHandle, RecycleAllocator};

pub struct ProcessControlBlock {
    // immutable
    pub pid: PidHandle,
    // mutable
    inner: UPSafeCell<ProcessControlBlockInner>,
}

pub struct ProcessControlBlockInner {
    // address space
    pub memory_set: MemorySet,
    // process tree
    pub parent: Option<Weak<ProcessControlBlock>>,
    pub children: Vec<Arc<ProcessControlBlock>>,
    pub is_zombie: bool,
    pub exit_code: i32,
    // file descriptors
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,
    // threads
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    pub task_res_allocator: RecycleAllocator,
    // signals
    pub signals: SignalFlags,
    /// the signal which is being handling
    pub handling_sig: isize,
    /// signal actions
    pub signal_actions: SignalActions,
    /// if the task is killed
    pub killed: bool,
    /// if the task is frozen by a signal
    pub frozen: bool,
}
