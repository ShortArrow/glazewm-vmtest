# Spike: what the Hyper-V thumbnail captures of the guest display

- Date: 2026-10-31
- Spike: "Guest screen capture" in docs/ROADMAP.md, for Hyper-V; `screendump` on QEMU was not part of it
- Host and guest: as in docs/spikes/2026-10-31-hyperv-wmi-control.md, restored from `daemon-task` and started, with the user `fixture` logged on automatically

## Method

`Msvm_VirtualSystemManagementService.GetVirtualSystemThumbnailImage` was called on the Hyper-V host through SSH, with the guest's realized `Msvm_VirtualSystemSettingData` and a width and height. The returned `ImageData` was read as RGB565 and saved as PNG. The guest's `Msvm_VideoHead` gave the resolution of the Hyper-V synthetic display.

The guest has two displays: Microsoft Hyper-V Video at 1024×768 and the Virtual Display Driver at 800×600.

For the RDP part, Remote Desktop was turned on in the guest (`fDenyTSConnections` = 0) without enabling a firewall rule, so port 3389 answered on the guest's loopback but not from the LAN. The client was FreeRDP 3.32.1 (`xfreerdp3` from the Ubuntu 24.04 archive, package `3.32.1+dfsg-0ubuntu0.24.04.1`) under Xvfb in a container, connecting at 1280×800 as `fixture` through an SSH port forward to the guest's `127.0.0.1:3389`. The sessions were read with `qwinsta` in the guest.

The thumbnail was taken in four states: before RDP, while RDP was connected, after the client was killed, and after `tscon 1 /dest:console` run as SYSTEM from a scheduled task.

## Observations

| Request | Result |
|---|---|
| 640×480 | Returned in 46 ms, 614 404 bytes, the desktop scaled down |
| 1024×768, the size of the synthetic display | Returned in 77–99 ms, 1 572 868 bytes |
| 1280×800, 1600×900, 1600×1000, 1920×1200 | Refused with return value 32775 in 4–5 ms |

| State | Sessions (`qwinsta`) | Thumbnail |
|---|---|---|
| Before RDP | `console` session 1, `fixture`, Active | The desktop with the taskbar |
| RDP connected | `rdp-tcp#0` session 1, `fixture`, Active; `console` session 3, Conn | The lock screen with the clock |
| RDP client killed | Session 1 Disc; `console` session 3, Conn | The lock screen, as while connected |
| After `tscon 1 /dest:console` | `console` session 1, `fixture`, Active | The desktop with the taskbar |

The thumbnail never showed the Virtual Display Driver's display, and nothing in the API selects a display.

## Consequences for the design

- The thumbnail shows only the Hyper-V synthetic display, at most at that display's resolution. A display added by a driver inside the guest, which is what the monitor scenarios add and remove, is not in it. A picture of every display has to be taken inside the session.
- While RDP owns the user's session, and after it disconnects until the session is returned to the console, the thumbnail shows the console's lock screen, not the session the tests run in. It still answers whether the console is at the lock screen or the desktop.
- `tscon <session> /dest:console` as SYSTEM returned the disconnected session to the console in the guest, with no input from the host.
- FreeRDP from the Ubuntu archive, run without a display under Xvfb, logged on to the guest through an SSH port forward with a password from standard input, with the guest's port 3389 closed to the LAN.

## Not measured

- Whether the synthetic display's resolution can be raised to fit larger thumbnails.
- Capture inside the session (by the guest daemon) and what it shows while RDP owns the session.
- The display topology in the guest while RDP is connected and after `tscon`.
- `screendump` on QEMU.
