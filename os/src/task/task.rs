use super::TaskContext;

#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    pub id: usize,
    pub task_status: TaskStatus,
    pub task_cx: TaskContext,
    pub priority: usize,
    pub pass: usize,
}

#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    UnInit,
    Ready,
    Running,
    Exited,
}