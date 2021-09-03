use nix::errno::Errno;
use nix::libc::c_int;
use nix::sys::signal::{killpg, signal, SigHandler};
use nix::unistd::Pid;

use lazy_static::lazy_static;
use nix::sys::wait::WaitStatus;
use nix::unistd::{self, pause, ForkResult};

use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

pub use nix::sys::signal::Signal;

lazy_static! {
    static ref GLOBAL_GID: AtomicI32 = {
        let pid = Pid::this();
        AtomicI32::new(pid.as_raw())
    };
    static ref GLOBAL_SIG: AtomicI32 = AtomicI32::new(Signal::SIGTERM as i32);
}

fn kill_global(check_err: bool) {
    let pid = Pid::from_raw(GLOBAL_GID.load(Ordering::SeqCst));
    let sig = unsafe { std::mem::transmute::<i32, Signal>(GLOBAL_SIG.load(Ordering::SeqCst)) };
    match killpg(pid, sig) {
        Err(err) if err != Errno::ESRCH => {
            if check_err {
                panic!("{}", err)
            }
        }
        _ => {}
    }
}

extern "C" fn quit_signal_handler(_signum: c_int) {
    unsafe { signal(Signal::SIGTERM, SigHandler::SigDfl) }.unwrap();
    kill_global(true);
}

extern "C" fn default_child_die_signal_handler(_signum: c_int) {
    let status_code = match nix::sys::wait::wait() {
        Ok(WaitStatus::Exited(_, st)) => st,
        _ => 0,
    };
    unsafe { signal(Signal::SIGTERM, SigHandler::SigDfl) }.unwrap();
    kill_global(false);
    unsafe { libc::exit((status_code & 0xff00) >> 8) };
}

pub enum RunAs {
    Simple,
    Fork,
}

fn generate_handler() -> Box<dyn Fn(Pid, Signal)> {
    Box::new(|gid: Pid, sig: Signal| {
        GLOBAL_GID.store(gid.as_raw(), Ordering::SeqCst);
        let signum = unsafe { std::mem::transmute::<Signal, i32>(sig) };
        GLOBAL_SIG.store(signum, Ordering::SeqCst);
        unsafe {
            signal(Signal::SIGINT, SigHandler::Handler(quit_signal_handler)).unwrap();
            signal(Signal::SIGQUIT, SigHandler::Handler(quit_signal_handler)).unwrap();
            signal(Signal::SIGTERM, SigHandler::Handler(quit_signal_handler)).unwrap();
            signal(
                Signal::SIGCHLD,
                SigHandler::Handler(default_child_die_signal_handler),
            )
            .unwrap();
        }
    })
}

fn exit_when_parent_or_child_dies(given_sig: Signal) {
    let gid = nix::unistd::getpgrp();
    GLOBAL_GID.store(gid.as_raw(), Ordering::SeqCst);
    let signum = unsafe { std::mem::transmute::<Signal, i32>(given_sig) };
    GLOBAL_SIG.store(signum, Ordering::SeqCst);

    unsafe {
        signal(
            Signal::SIGCHLD,
            SigHandler::Handler(default_child_die_signal_handler),
        )
        .unwrap();
    }

    #[cfg(target_os = "linux")]
    {
        unsafe {
            signal(Signal::SIGHUP, SigHandler::Handler(quit_signal_handler)).unwrap();
        }
        prctl::set_death_signal(Signal::SIGHUP as isize).unwrap();
        loop {
            pause();
        }
        return;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let interval = Duration::from_secs(5);
        loop {
            let pid = nix::unistd::getppid();
            if pid.as_raw() == 1 {
                unsafe { signal(Signal::SIGTERM, SigHandler::SigDfl) }.unwrap();
                killpg(gid, given_sig).unwrap();
                unsafe { libc::exit(0) };
            }

            std::thread::sleep(interval);
        }
    }
}

#[allow(dead_code)]
fn simple(given_sig: Signal) -> std::io::Result<()> {
    let handler = generate_handler();
    let gid = Pid::this();
    handler(gid, given_sig);
    return Ok(());
}

pub fn fork(given_sig: Signal) -> std::io::Result<()> {
    let handler = generate_handler();
    match unsafe { unistd::fork().unwrap() } {
        ForkResult::Parent { child, .. } => {
            handler(child, given_sig);
            loop {
                pause();
            }
        }
        ForkResult::Child => {
            GLOBAL_GID.store(Pid::this().as_raw(), Ordering::SeqCst);
            GLOBAL_SIG.store(Signal::SIGTERM as i32, Ordering::SeqCst);
        }
    }
    nix::unistd::setpgid(Pid::from_raw(0), Pid::from_raw(0)).unwrap();
    match unsafe { unistd::fork().unwrap() } {
        ForkResult::Parent { .. } => {
            exit_when_parent_or_child_dies(given_sig);
        }
        ForkResult::Child => {
            GLOBAL_GID.store(Pid::this().as_raw(), Ordering::SeqCst);
            GLOBAL_SIG.store(Signal::SIGTERM as i32, Ordering::SeqCst);
        }
    }

    Ok(())
}
