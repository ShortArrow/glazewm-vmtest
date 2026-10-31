//! A Windows guest on Hyper-V, restored through WMI on the Hyper-V host and reached over SSH and the guest daemon.

use std::process::Command;

use vmtest_runner::Hypervisor;

use crate::powershell::{encoded_command, literal};
use crate::windows_guest::run;

/// Where the Hyper-V host's PowerShell runs.
#[derive(Clone, Debug)]
pub enum HyperVHost {
    /// On the machine the runner runs on.
    Local,
    /// On another machine, reached by `ssh <destination>` with a key; the user must be allowed to use Hyper-V there.
    Ssh(String),
}

/// A Hyper-V guest, restored to a checkpoint taken with the guest off.
#[derive(Clone, Debug)]
pub struct HyperVCheckpoint {
    pub host: HyperVHost,
    pub vm_name: String,
    pub checkpoint: String,
}

impl Hypervisor for HyperVCheckpoint {
    fn restore(&mut self) -> Result<(), String> {
        run(&mut powershell(
            &self.host,
            &restore_script(&self.vm_name, &self.checkpoint),
        ))
    }
}

/// The command that runs `script` in Windows PowerShell on `host`.
fn powershell(host: &HyperVHost, script: &str) -> Command {
    let mut command = match host {
        HyperVHost::Local => Command::new("powershell"),
        HyperVHost::Ssh(destination) => {
            let mut ssh = Command::new("ssh");
            ssh.args(["-o", "BatchMode=yes", destination, "powershell"]);
            ssh
        }
    };
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-EncodedCommand",
        &encoded_command(script),
    ]);
    command
}

/// `guests/hyperv-windows/restore-checkpoint.ps1` invoked on `vm_name` and `checkpoint`.
fn restore_script(vm_name: &str, checkpoint: &str) -> String {
    format!(
        "& {{\n{RESTORE_CHECKPOINT}\n}} -VMName {} -Checkpoint {}",
        literal(vm_name),
        literal(checkpoint)
    )
}

const RESTORE_CHECKPOINT: &str =
    include_str!("../../../guests/hyperv-windows/restore-checkpoint.ps1");

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn invokes_the_restore_script_with_the_guest_and_checkpoint_as_literals() {
        let script = restore_script("vmtest-win11", "it's");

        assert!(script.starts_with("& {"), "{script}");
        assert!(script.contains("ApplySnapshot"), "{script}");
        assert!(
            script.ends_with("} -VMName 'vmtest-win11' -Checkpoint 'it''s'"),
            "{script}"
        );
    }

    #[test]
    fn the_restore_fits_on_a_command_line_that_cmd_accepts() {
        let command = powershell(
            &HyperVHost::Ssh("admin@hyperv".into()),
            &restore_script("vmtest-win11", "daemon-task"),
        );

        let length: usize = arguments(&command).iter().map(|a| a.len() + 1).sum();
        assert!(length < 8191, "{length} characters");
    }

    #[test]
    fn runs_powershell_on_the_runners_machine() {
        let command = powershell(&HyperVHost::Local, "dir");

        assert_eq!(command.get_program(), "powershell");
        assert_eq!(
            arguments(&command),
            [
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                "ZABpAHIA"
            ]
        );
    }

    #[test]
    fn runs_powershell_on_a_remote_host_through_ssh_without_prompting() {
        let command = powershell(&HyperVHost::Ssh("admin@hyperv".into()), "dir");

        assert_eq!(command.get_program(), "ssh");
        assert_eq!(
            arguments(&command),
            [
                "-o",
                "BatchMode=yes",
                "admin@hyperv",
                "powershell",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                "ZABpAHIA"
            ]
        );
    }
}
