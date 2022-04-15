# Process

## 0x00 The Concepts and Syscalls

It's hard to define what a `process` is. Usually it is a procedure in which the OS selects an executable file and performs the dynamic execution. During the execution there will be many interaction between the process and  the hardware or virtual resources, and we know those are handled by the OS through syscalls. Besides, there are some important syscalls specially made for process management: `fork/exec/waitpid`:

- `fork`: When a process (let's name it A) call fork, the kernel will create a new process (let's name it B) which is almost identical to A: they have exactly same stack, .text segment or other data segment content, and every registers except `a0` which stores the return value of the syscall. They are in different address spaces but the bytes stores in these address space are exactly same at the moment `fork` returns. The only way a process can figure out whether it is the new process or the old parent is the returen value of `fork`: 0 for the new born process and `pid` of child process for the parent. This parent-child relation is very important in unix-like OS.
- `exec`: This will help us run a new program in the current address space, use it together with `fork` we can easily create a process that runs a new program. 
- `waitpid`: When a process returns, the memory resources it have consumed cannot be fully recycled through the `exit` syscall, for eaxmple the current `kernel stack`. A typical solution for this is to mark the process as `zombie`, then it's parent process do the rest recycle work and get the exit status through the `waitpid` syscall. 

## 0x01 Data Structures for Process

`RAII` is heavily used to help us safe memory management. For a process, we bind its `pid`, `kernel stack`, and `address space(MemorySet)` to a `TaskControlBlock`. The TCBs ares stored in a tree formed by the parent-child relations(created through fork&exec) between processes:

```rust
pub struct TaskControlBlock {
    // immutable
    pub pid: PidHandle,
    pub kernel_stack: KernelStack,
    // mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

pub struct TaskControlBlockInner {
    pub trap_cx_ppn: PhysPageNum,
    pub base_size: usize,
    pub priority: usize,
    pub pass: usize,
    pub task_cx: TaskContext,
    pub task_status: TaskStatus,
    pub memory_set: MemorySet,
    pub parent: Option<Weak<TaskControlBlock> >,
    pub children: Vec<Arc<TaskControlBlock> >,
    pub exit_code: i32,
}
```

Here we use `alloc::sync::Weak` to wrap the `parent` pointer so that there will be no cyclic refernces between the parent and it's child. 

Another significant modification from previous chapter is that we split the original task manager module into `Processor` and `TaskManager`. 
- The `Processor` module maintains the CPU's states, including current process and idle task context. In a single-core CPU environment, we only create one gloable instance of `Processor`.
- The `TaskManager` stores all `Arc`s in a `BTreeMap`, so that we can easily fetch/remove or add/insert tasks(processes) with our scheduler.

```rust
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
        self.scheduler.insert_task(task.getpid(), task.clone().inner_exclusive_access().pass);
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
```

## 0x02 Process Management

Note that we need to manually set a root process which derives all other ones. Such root process is usually called `initproc`. In rCore the `initproc` is implemented as follows:

```rust
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{exec, fork, wait, yield_};

#[no_mangle]
fn main() -> i32 {
    if fork() == 0 {
        exec("user_shell\0");
    } else {
        loop {
            let mut exit_code: i32 = 0;
            let pid = wait(&mut exit_code);
            if pid == -1 {
                yield_();
                continue;
            }
            println!(
                "[initproc] Released a zombie process, pid={}, exit_code={}",
                pid, exit_code,
            );
        }
    }
    0
}
```

The `iniproc` process is the first task runned by os after started, and it only has two tasks: 

1. Create the `shell` process
2. Adopt all the orphan process, wait for them to exit and recycle the resources

Although the `initproc` is very special, it is still just a user mode process. 

Then we implemented the fork/exit/waitpid syscalls according to their semantics.

```rust
pub fn sys_fork() -> isize {
    let current = current_task().unwrap();
    let child_task = current.fork();
    let child_pid = child_task.pid.0;
    // modify return address in trap context
    let trap_cx = child_task.inner_exclusive_access().get_trap_cx();
    // return value of child is 0
    trap_cx.x[10] = 0;  //x[10] is a0 reg
    // add task to scheduler queue
    add_task(child_task);
    child_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

// If pid == -1, try to recycle every child
// If there is not a child process whose pid is same as given, return -1.
// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    // no such child
    if inner.children
        .iter()
        .find(|p| {pid == -1 || pid as usize == p.getpid()})
        .is_none() {
        return -1;
    }
    // get child
    let pair = inner.children
        .iter()
        .enumerate()
        .find(|(_, p)| {
            // ++++ temporarily access child PCB exclusively
            p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
            // ++++ stop exclusively accessing child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after removing from children list
        // so that after dropped, the kernel stack and pagetable and pid_handle will be recycled
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        let exit_code = child.inner_exclusive_access().exit_code;
        // write exit_code to the user space
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
}
```

The corresponding apis of `TaskControlBlock` are as follows:

```rust
impl TaskControlBlock {
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }
    
    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    pub fn new(elf_data: &[u8]) -> Self {
        // map user space memory set
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        let task_status = TaskStatus::Ready;
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let kernel_stack_top = kernel_stack.get_top();
         // push a task context which goes to trap_return to the top of kernel stack
        let task_control_block = Self {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe { UPSafeCell::new(TaskControlBlockInner {
                trap_cx_ppn,
                base_size: user_sp,
                priority: 16,
                pass: 0,
                task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                task_status,
                memory_set,
                parent: None,
                children: Vec::new(),
                exit_code: 0,
            })},
        };
        // prepare TrapContext in user space
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        task_control_block
    }

    pub fn exec(&self, elf_data: &[u8]) {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        // substitute memory_set
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        let mut inner = self.inner_exclusive_access();
        inner.memory_set = memory_set;
        // update trap_cx ppn
        inner.trap_cx_ppn = trap_cx_ppn;
        // initialize trap_cx
        let trap_cx = inner.get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize,
        );
    }
    pub fn fork(self: &Arc<TaskControlBlock>) -> Arc<TaskControlBlock> {
        // get parent PCB
        let mut parent_inner = self.inner.exclusive_access();
        // make a copy of memory space 
        let memory_set = MemorySet::from_existed_userspace(
            &parent_inner.memory_set);
        // allocate a pid and kernel stack
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let kernel_stack_top = kernel_stack.get_top();
        // get trap context ppn
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        // create inner
        let task_control_block_inner = unsafe { UPSafeCell::new(TaskControlBlockInner { 
            trap_cx_ppn: trap_cx_ppn, 
            base_size: parent_inner.base_size, 
            priority: parent_inner.priority, 
            pass: parent_inner.pass, 
            task_cx: TaskContext::goto_trap_return(kernel_stack_top), 
            task_status: TaskStatus::Ready, 
            memory_set, 
            parent: Some(Arc::downgrade(self)), 
            children: Vec::new(), 
            exit_code: 0, 
        })};
        // modify kernel sp in child's trap_cx
        let trap_cx = task_control_block_inner.exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kernel_stack_top;
        // create child's PCB
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: task_control_block_inner,
        });
        // add child to parent
        parent_inner.children.push(task_control_block.clone());
        // return
        task_control_block
    }
}
```

Then we implement the main task management functions:

```rust
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
```

## References

[rCore-Tutorial-Book-v3/chapter5](https://rcore-os.github.io/rCore-Tutorial-Book-v3/chapter5/index.html)