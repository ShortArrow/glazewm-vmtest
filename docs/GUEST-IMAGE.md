# Guest image

The Windows guest is a Windows 11 Pro install plus a chain of snapshots. The harness restores the last one: `ssh-key-only` on Hyper-V, `daemon-task` on dockur/windows. The snapshots are not in git; this page is how to rebuild them, on Hyper-V and on dockur/windows.

## Snapshots

Each snapshot is the one above it plus the listed change.

| Snapshot | Adds |
|---|---|
| `clean` | Unattended Windows 11 Pro install; user `fixture` logs on automatically; sshd enabled by `guests/dockur-windows/oem/setup-ssh.ps1` with the key in `guests/dockur-windows/.secrets/`, accepting that key only |
| `uac-on` | dockur/windows only, whose install turns UAC off: `EnableLUA=1` in `HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System`, then a reboot. The Hyper-V install leaves UAC on, so `vdd` follows `clean` there |
| `vdd` | VirtualDrivers/Virtual-Display-Driver `25.7.23`: its signer certificate in `LocalMachine\TrustedPublisher`, a `Root\MttVDD` device, the driver installed with `pnputil`, `C:\VirtualDisplayDriver\vdd_settings.xml` |
| `vcredist` | Microsoft Visual C++ 2015–2022 x64 redistributable |
| `daemon-task` | Scheduled task `vmtest-guest-daemon` running `C:\vmtest\vmtest-guest-daemon.exe 127.0.0.1:7041` as `fixture`, interactive logon type, `RunLevel Limited`, no trigger. No firewall rule opens TCP 7041; the harness reaches the daemon through an SSH port forward. dockur/windows runs the daemon without the argument, which listens on the same address by default |
| `rdp` | Hyper-V only. Remote Desktop connections allowed (`fDenyTSConnections=0`) with every rule of the Remote Desktop firewall group left disabled, so TCP 3389 answers only on the guest itself and an RDP client reaches it through an SSH port forward. Scheduled task `vmtest-tscon` running as SYSTEM, no trigger, which moves `fixture`'s session to the console with `tscon` |
| `ssh-key-only` | Hyper-V only. sshd accepts public keys only, set by `guests/dockur-windows/oem/ssh-key-only.ps1`. `setup-ssh.ps1` runs the same script, so on a guest installed from the current answer ISO this snapshot changes nothing |

## Credentials

Both platforms use the same ones, in guests/dockur-windows:

- `.env` with `GUEST_PASSWORD=<password>`, the password of `fixture`.
- `.secrets/id_ed25519`, the key the harness connects with; its public key copied to `oem/authorized_key.pub`. On a Windows runner, OpenSSH ignores a private key that other users can read, which is the case for a file that inherits the permissions of a drive root such as `C:\`; `icacls .secrets\id_ed25519 /inheritance:r /grant:r "${env:USERNAME}:F"` in PowerShell, or with `%USERNAME%` in cmd, leaves only its owner.
- `.secrets/known_hosts`, the guest's host key, recorded by the harness the first time it connects and checked on every later connection. After a reinstall the guest has a new host key, and its line in this file has to be removed.

## Rebuilding on Hyper-V

The guest is the virtual machine `vmtest-win11` on the harness's Hyper-V host.

1. On the host, create an external virtual switch named `External` on the network adapter the host is reached through, so the guest gets an address from the same network and can reach the internet.
2. Build the answer-file ISO with `guests/hyperv-windows/build-answer-iso.ps1`, from `guests/hyperv-windows/answer/autounattend.xml`, `oem/setup-ssh.ps1`, `oem/authorized_key.pub`, `GUEST_PASSWORD`, and the `/IMAGE/NAME` of Pro in the installation media (`Windows 11 Pro`).
3. Create the virtual machine: generation 2, a local key protector and a virtual TPM, Secure Boot with the `MicrosoftWindows` template, 4 processors, 8 GB of static memory, a new 64 GB VHDX, the `External` switch, automatic checkpoints off. Fix the synthetic display at 1280×800 with `Set-VMVideo -ResolutionType Single -HorizontalResolution 1280 -VerticalResolution 800`; dockur/windows gives its display the same size, so a scenario sees the same displays on both platforms and one baseline serves both. Attach the Windows 11 installation ISO and the answer-file ISO as two DVD drives, and boot from the first.
4. Start it and press a key at "Press any key to boot from CD or DVD", through `Msvm_Keyboard.TypeKey` in `root\virtualization\v2`. Do not press one after the first restart, so the guest boots from its disk. Wait until `ssh fixture@<guest address>` answers; `Get-VMNetworkAdapter` lists the address.
5. For `vdd`, `vcredist`, `daemon-task`, `rdp` and `ssh-key-only` in that order, copy what the step needs into the guest and run, over SSH:
   - `vdd`: `guests/hyperv-windows/install-vdd.ps1 -DriverFolder <folder>`, with the `VirtualDisplayDriver` folder of the release asset `VirtualDisplayDriver-x86.Driver.Only.zip` (SHA-256 `e24210692b442b39af763536330ce78b423f19342b7a7792c26de3944e418b3a`).
   - `vcredist`: `guests/hyperv-windows/install-vcredist.ps1`.
   - `daemon-task`: `guests/hyperv-windows/install-daemon-task.ps1 -User fixture`.
   - `rdp`: `guests/hyperv-windows/install-rdp.ps1 -User fixture`.
   - `ssh-key-only`: `guests/dockur-windows/oem/ssh-key-only.ps1`.
6. After `clean` and after each step, remove what was copied in, shut the guest down from inside (`shutdown /s /t 0`), and take a standard checkpoint with `Checkpoint-VM -SnapshotName <name>` once the virtual machine is off.

## Rebuilding on dockur/windows

The guest is defined by guests/dockur-windows/compose.yaml, and the snapshots live in the qcow2 disk in the Docker volume `glazewm-vmtest-win11-storage`.

1. `docker compose up -d` in guests/dockur-windows and wait until `ssh -p 2222 fixture@127.0.0.1` answers.
2. For each row of the table: make the change over SSH, shut the guest down from inside (`shutdown /s /t 0`), and create the snapshot with `qemu-img snapshot -c <name> /storage/data.qcow2` from the `dockurr/windows` image, as the dockur-windows skill describes. The scripts in guests/hyperv-windows make the changes for `vdd` and `vcredist`.
