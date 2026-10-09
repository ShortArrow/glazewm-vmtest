//! Runs the SUT's command-line client as a child process.

use std::path::PathBuf;
use std::process::Command;

use crate::{CommandOutput, GlazewmCli};

/// Runs the `glazewm-cli` executable at a fixed path.
pub struct ProcessGlazewmCli {
    pub executable: PathBuf,
}

impl GlazewmCli for ProcessGlazewmCli {
    fn run(&self, args: &[String]) -> Result<CommandOutput, String> {
        let mut command = Command::new(&self.executable);
        command.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            /// `CREATE_NO_WINDOW`: a console child of a windowless daemon would otherwise open a console window on the desktop under test.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let output = command
            .output()
            .map_err(|error| format!("cannot start {}: {error}", self.executable.display()))?;
        Ok(CommandOutput {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}
