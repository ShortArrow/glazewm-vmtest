# Guest image

The Windows guest for dockur/windows is defined by guests/dockur-windows/compose.yaml and the snapshots in its qcow2 disk. The snapshots live in the Docker volume `glazewm-vmtest-win11-storage`, not in git; this page is how to rebuild them.

## Snapshots

Each snapshot is the one above it plus the listed change. The harness restores `daemon-task`.

| Snapshot | Adds |
|---|---|
| `clean` | Unattended Windows 11 Pro install; user `fixture` logs on automatically; sshd enabled by `oem/setup-ssh.ps1` with the key in `.secrets/` |
| `uac-on` | `EnableLUA=1` in `HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System`, then a reboot |
| `vdd` | VirtualDrivers/Virtual-Display-Driver `25.7.23`: its signer certificate in `LocalMachine\TrustedPublisher`, a `Root\MttVDD` device, the driver installed with `pnputil`, `C:\VirtualDisplayDriver\vdd_settings.xml` |
| `vcredist` | Microsoft Visual C++ 2015–2022 x64 redistributable |
| `daemon-task` | Firewall rule `vmtest-guest-daemon` allowing TCP 7041 inbound; scheduled task `vmtest-guest-daemon` running `C:\vmtest\vmtest-guest-daemon.exe` as `fixture`, interactive logon type, `RunLevel Limited`, no trigger |

## Rebuilding

1. Create `guests/dockur-windows/.env` with `GUEST_PASSWORD=<password>`, generate `.secrets/id_ed25519`, and copy its public key to `oem/authorized_key.pub`.
2. `docker compose up -d` in guests/dockur-windows and wait until `ssh -p 2222 fixture@127.0.0.1` answers.
3. For each row above: make the change over SSH, shut the guest down from inside (`shutdown /s /t 0`), and create the snapshot with `qemu-img snapshot -c <name> /storage/data.qcow2` from the `dockurr/windows` image, as the dockur-windows skill describes.

The commands used for each step are in docs/spikes/.
