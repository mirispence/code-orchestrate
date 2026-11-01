//! Settings › Data › Open data folder (spec §9.4): the folder opens in the
//! system file manager.
use std::cell::RefCell;
use std::io;

use code_orchestrate_lib::launch::{Spawner, TerminalCommand, open_folder};

#[derive(Default)]
struct FakeSpawner {
    spawned: RefCell<Vec<TerminalCommand>>,
}

impl Spawner for FakeSpawner {
    fn spawn(&self, cmd: &TerminalCommand) -> io::Result<()> {
        self.spawned.borrow_mut().push(cmd.clone());
        Ok(())
    }
}

#[test]
fn opens_the_folder_in_the_platform_file_manager() {
    let spawner = FakeSpawner::default();
    let dir = if cfg!(windows) { r"C:\Users\me\AppData\Roaming\code-orchestrate" } else { "/home/me/.local/share/code-orchestrate" };

    open_folder(&spawner, dir).unwrap();

    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    assert_eq!(
        spawner.spawned.into_inner(),
        vec![TerminalCommand { program: program.into(), args: vec![dir.into()], current_dir: None, env: None }]
    );
}
