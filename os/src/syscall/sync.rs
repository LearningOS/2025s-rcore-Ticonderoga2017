use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::syscall::thread::sys_gettid;
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec;
/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id
        // id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() - 1
        // process_inner.mutex_list.len() as isize - 1
    };

    if id >= process_inner.mtx_available.len() {
        process_inner.mtx_available.resize(id + 1, 0);
    }
    process_inner.mtx_available[id] = 1;

    id as isize
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    
    process_inner.init_mtx_allocations();
    process_inner.init_mtx_need();
    let tid = sys_gettid() as usize;
    process_inner.mtx_need[tid][mutex_id] = 1;
    if process_inner.deadlock_det {
        let mut work = process_inner.mtx_available.clone();

        let task_count = process_inner.thread_count();
        let mtx_count = process_inner.mtx_count();
        let mut finish = vec![false; task_count];
        loop {
            let mut found = false;
            for i in 0..task_count {                
                if !finish[i] 
                && process_inner.mtx_need[i].iter().enumerate().all(|(j, &x)| x <= work[j]) {
                    for j in 0..mtx_count {
                        work[j] += process_inner.mtx_allocation[i][j];
                    }
                    finish[i] = true;
                    found = true;
                }
            }
            if !found {
                break;
            }
        }
        if !finish.iter().all(|&x| x) {
            process_inner.mtx_need[tid][mutex_id] = 0;
            return -0xdead;
        }
    }

    drop(process_inner);
    drop(process);
    mutex.lock();

    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    process_inner.mtx_available[mutex_id] -= 1;
    process_inner.mtx_allocation[tid][mutex_id] = 1;
    process_inner.mtx_need[tid][mutex_id] = 0;

    0
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.unlock();

    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let task_id = sys_gettid() as usize;
    if process_inner.mtx_allocation[task_id][mutex_id] > 0 {
        process_inner.mtx_allocation[task_id][mutex_id] -= 1;
        process_inner.mtx_available[mutex_id] += 1;
    }   

    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };

    if id >= process_inner.sem_available.len() {
        process_inner.sem_available.resize(id + 1, 0);
    }
    process_inner.sem_available[id] = res_count;

    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    sem.up();

    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let task_id = sys_gettid() as usize;
    if process_inner.sem_allocations[task_id][sem_id] > 0 {
        process_inner.sem_allocations[task_id][sem_id] -= 1;
        process_inner.sem_available[sem_id] += 1;
    }    

    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());

    process_inner.init_sem_allocations();
    process_inner.init_sem_need();
    let task_id = sys_gettid() as usize;
    process_inner.sem_need[task_id][sem_id] += 1;
    if process_inner.deadlock_det && sem_id != 0 {
        let mut work = process_inner.sem_available.clone();
        // banker
        let task_count = process_inner.thread_count();
        let sem_count = process_inner.sem_count();
        let mut finish = vec![false; task_count];
        loop {
            let mut found = false;
            for i in 0..task_count {
                if !finish[i] 
                && process_inner.sem_need[i].iter().enumerate().all(|(j, &x)| x <= work[j]) {
                    for j in 0..sem_count {
                        work[j] += process_inner.sem_allocations[i][j];
                    }
                    finish[i] = true;
                    found = true;
                }
            }
            if !found {
                break;
            }
        }
        if !finish.iter().all(|&x| x) {
            process_inner.sem_need[task_id][sem_id] -= 1;
            return -0xdead;
        }
    }
    drop(process_inner);
    sem.down();
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    process_inner.sem_need[task_id][sem_id] -= 1;
    if process_inner.sem_available[sem_id] > 0 {
        process_inner.sem_available[sem_id] -= 1;
        process_inner.sem_allocations[task_id][sem_id] += 1;
    }
    0
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}
/// enable deadlock detection syscall
///
/// YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(_enabled: usize) -> isize {
    match _enabled {
        0 => {
            current_process().inner_exclusive_access().deadlock_det = false;
            0
        }
        1 => {
            current_process().inner_exclusive_access().deadlock_det = true;
            0
        }
        _ => -1
    }
}
