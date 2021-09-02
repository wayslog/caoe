CaoE - Kill all children processes when the parent dies
=================

CaoE makes it easy to automatically kills all spawned children (and grandchildren) processes when the parent dies, even if killed by SIGKILL, and was rewritten in Rust.

## Usage

```rust
use nix::sys::signal::Signal;

// General usage, run the process in fork-and-fork model.
caoe::fork(Signal::SIGTERM).unwrap();

```

## How it works

When `caoe::fork()` is called, it forks out a child process and a grandchild process. Both the parent and the child process will block, only the grandchild process will continue to run. The child process keeps checking the status of parent. If it found that the parent has died, it kills grandchild process (and grand-grandchild processes if there are any) and suicides.
