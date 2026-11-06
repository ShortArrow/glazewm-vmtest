# CI

Two GitHub Actions workflows check the harness.

| Workflow | Trigger | Runs on | Checks |
|---|---|---|---|
| `.github/workflows/ci.yml` | Every push to `main` and every pull request | `windows-latest` | `cargo fmt --check`, `cargo clippy -D warnings`, and the tests that need no guest |
| `.github/workflows/live.yml` | Every push to `main`, and by hand | `windows-latest` for the builds, then the self-hosted runner `vmtest-kvm` | Every live scenario against the dockur/windows guest, once for each glazewm below |

## The live workflow

The Windows jobs build the guest daemon, the test-window application and the load generator once, and glazewm once for each of these commits, with the nightly toolchain the workflow pins:

| glazewm | Repository and commit | Expectations |
|---|---|---|
| `upstream` | glzr-io/glazewm `5709ad0a`, the commit the baselines are recorded from | `ci/live-expectations/upstream.txt` |
| `all-fixes` | ShortArrow/glazewm `vmtest/all-fixes` at `f8dc1eba`, upstream with every fix the harness has led to | `ci/live-expectations/all-fixes.txt` |

The live job then runs `ci/run-live.sh` with each expectations file in turn on the runner. The script runs every live scenario on its own, one after another on the one guest. A scenario listed as `fails` has to fail, one listed as `varies` is not run, and every other scenario has to pass. A scenario that reproduces a bug glazewm still has upstream is listed as `fails` in `upstream.txt` and passes on `all-fixes`, so one run shows the bug and its fix.

The live workflow has no `pull_request` trigger, so a pull request from a fork never runs on the self-hosted runner.

## The self-hosted runner

The runner is the Docker host of docs/GUEST-IMAGE.md, `vmtest-kvm`, with the dockur/windows guest built on it. It runs as the user `runner`, which belongs to the `docker` group and cannot use `sudo`; the steps below run as `vmtest`.

1. Install the build tools: `sudo apt-get install -y build-essential pkg-config curl`.
2. Create the user: `sudo useradd --create-home --shell /bin/bash --groups docker runner` and `sudo passwd -l runner`.
3. Install Rust for that user: as `runner`, `curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.97.1`.
4. Give it the guest's password and SSH key, readable by `runner` alone, in `/home/runner/vmtest-secrets`: `.env` from guests/dockur-windows, and the private key that guests/dockur-windows/.secrets holds as `id_ed25519`. The live job copies both into its checkout and removes them when it ends.
5. Download the runner from https://github.com/actions/runner/releases into `/home/runner/actions-runner`, check it against the SHA-256 in the release notes, and run `sudo /home/runner/actions-runner/bin/installdependencies.sh`.
6. Register it with a registration token from the repository's Settings → Actions → Runners → New self-hosted runner, as `runner` in `/home/runner/actions-runner`: `./config.sh --unattended --url https://github.com/ShortArrow/glazewm-vmtest --token <token> --name vmtest-kvm --labels vmtest-kvm`.
7. Install it as a service that runs as `runner`: `sudo ./svc.sh install runner` and `sudo ./svc.sh start` in `/home/runner/actions-runner`.

The runner reaches GitHub over outbound HTTPS and listens on no port.
