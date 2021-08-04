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

fn parent(path: String, pause: bool, fork: bool) {
    let params = TestParams { path, pause, fork };

    if params.fork {
        caoe::fork(caoe::Signal::SIGTERM).unwrap();
    } else {
        caoe::simple(caoe::Signal::SIGTERM).unwrap();
    }

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
    });
    match parent.join_timeout(Duration::from_secs(10)) {
        Ok(_) => {}
        Err(_) => {}
    }
}

#[test]
fn test_all_child_processes_should_be_killed_if_parent_quit_normally() {
    init_test_env();
    let tempdir = tempdir().unwrap();
    let dir = tempdir.path().to_str().unwrap().to_string();
    parent(dir, false, false);
}
