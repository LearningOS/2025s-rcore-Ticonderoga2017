//! Implementation of [`SyscallCounter`]

#[derive(Copy, Clone)]
#[repr(C)]
/// Syscal counter structure containing syscall id and count
pub struct SyscallCounter {
    /// Syscall id
    pub syscall_id: usize,
    /// Syscall count
    pub count: isize,
}

impl SyscallCounter {
    /// Create a new empty syscall counter
    pub fn init_counter() -> Self {
        Self {
            syscall_id: 0,
            count: 0,
        }
    }
}
