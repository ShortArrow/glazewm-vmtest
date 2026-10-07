# Spike: monitor changes through an IddCx virtual display driver on dockur/windows

- Date: 2026-10-07
- Spike: "In-guest virtual display driver (IddCx)" in docs/ROADMAP.md
- Guest: snapshot `uac-on` of the guest described in docs/spikes/2026-10-07-interactive-session-dockur.md. Its own display is a single 1280×800 output of the Red Hat VirtIO GPU DOD controller.
- Driver: VirtualDrivers/Virtual-Display-Driver release `25.7.23`, asset `VirtualDisplayDriver-x86.Driver.Only.zip` (an amd64 package despite the name), SHA-256 `e24210692b442b39af763536330ce78b423f19342b7a7792c26de3944e418b3a`, matching the digest GitHub publishes. MIT licence. Catalog signed by SignPath Foundation, issued by GlobalSign GCC R45 CodeSigning CA 2020. No build provenance attestation is published.

## Method

- A hidden top-level window in session 1, created the way glazewm creates its listener window, logged every `WM_DISPLAYCHANGE`, `WM_SETTINGCHANGE` with `SPI_SETWORKAREA`, and `WM_DEVICECHANGE` with `DBT_DEVNODES_CHANGED`, together with the monitor list from `EnumDisplayMonitors` at that moment.
- The driver was installed over SSH: a root-enumerated device with hardware ID `Root\MttVDD` created through SetupAPI, then `pnputil /add-driver /install`.
- The monitor count was changed by writing `SETDISPLAYCOUNT <n>` to the named pipe `MTTVirtualDisplayPipe`, and the device was disabled and enabled with `Disable-PnpDevice` / `Enable-PnpDevice`.

## Observations

| Operation | Observed |
|---|---|
| Install without user interaction | Failed with `0xE0000242` until the signer certificate was added to `LocalMachine\TrustedPublisher` |
| First install | 1 → 2 monitors; all three messages arrived; the new monitor was `DISPLAY2` at 1280,0, 800×600 |
| `SETDISPLAYCOUNT 3`, `2`, `1` | Every change removed all virtual monitors, then created the new set about 0.3–0.5 s later. Each change delivered 10–15 messages, some of them while only the base monitor existed. Totals after settling were base + n |
| `SETDISPLAYCOUNT 0` | Treated as 1, as the driver source does (`if (monitorcount == 0) monitorcount = 1`) |
| Device names | A new name on every re-creation: `DISPLAY3`–`5`, then `6`–`7`, `8`, `9` |
| Disable / enable the device | All virtual monitors removed / added together: 2 → 1 → 2, about 5 messages each |

## Consequences for the design

- The messages glazewm listens for do arrive for virtual-monitor changes, in a dockur guest, without host input.
- This driver changes the number of monitors, not individual monitors: removing only the middle one of three (UC-1) cannot be expressed, and every change passes through an intermediate one-monitor topology.
- The base display of the platform stays; all displays lost (UC-6) needs another mechanism.

## Not measured

- Arrangement, resolution, or primary changes of the virtual monitors.
- glazewm's own response; glazewm was not running.
