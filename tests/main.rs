use std::sync::Once;
use tempfile::tempdir;

use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::time::Duration;

static PROC: Once = Once::new();

fn init_test_env() {
    PROC.call_once(move || {
        procspawn::init();
    });
}

fn is_process_alive(pid: &u32) -> bool {
    match unsafe { libc::kill(*pid as libc::pid_t, 0 as libc::c_int) } {
        0 => true,
        -1 => {
            use nix::errno::errno;
            let errno_int = errno();
            match errno_int {
                0 => return true,
                1 => return true,
                3 => return false,
                x => {
                    panic!("fail to kill errno: {}", x);
                }
            }
        }
        x => {
            panic!("fail to kill ret code: {}", x)
        }
    }
}

fn child(path: String) {
    let fname = format!("{}/child-{}", &path, Pid::this().as_raw());
    File::create(&fname).unwrap();
    std::thread::sleep(Duration::from_secs(100));
}

#[derive(Serialize, Deserialize)]
struct TestParams {
    path: String,
    pause: bool,
    fork: bool,
}

fn run_parent(path: String, pause: bool, fork: bool) {
    let target_path = dbg!(path.clone());
    let params = TestParams { path, pause, fork };

    caoe::fork(caoe::Signal::SIGTERM).unwrap();

    let parent = procspawn::spawn(params, |params| {
        let fname = format!("{}/parent-{}", &params.path, Pid::this().as_raw());
        File::create(&fname).unwrap();
        let mut procs = Vec::new();
        for _ in 0..3 {
            let path_str = params.path.clone();
            let proc = procspawn::spawn(path_str, |path_str| child(path_str));
            procs.push(proc);
        }

        if params.pause {
            std::thread::sleep(Duration::from_secs(10));
        } else {
            std::thread::sleep(Duration::from_millis(100));
        }
        println!("baka");
    });
    let ppid = parent.pid().unwrap();
    match parent.join_timeout(Duration::from_secs(10)) {
        Ok(_) => {}
        Err(_) => {}
    }
    assert!(!is_process_alive(&ppid));
    let sub_items = glob::glob(&format!("{}/*", target_path)).unwrap().count();
    assert_eq!(sub_items, 4);

    let pids = vec![];

    for _ in 0..6 {
        if pids.iter().any(is_process_alive) {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    assert_eq!(
        glob::glob(&format!("{}/{}", target_path, "parent-*"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn test_all_child_processes_should_be_killed_if_parent_quit_normally() {
    init_test_env();
    let tempdir = tempdir().unwrap();
    let dir = tempdir.path().to_str().unwrap().to_string();
    run_parent(dir, false, true);
}
