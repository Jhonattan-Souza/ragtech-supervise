# Agent Notes

## Shape

- Docker/shell repo; no package manager, lockfile, or unit-test config at root.
- Root `Dockerfile` downloads `supervise-${SUPERVISE_VERSION}.sh`.
- Main image builds need network access to Ragtech.
- `nut-bridge/` is a separate optional image; it reads Supervise SQLite and exposes NUT on `3493`.
- `init.sh` links `/data/monit.db*` into `/opt/supervise` and enables SQLite WAL.
- Live DB consumers need `monit.db`, `monit.db-wal`, and `monit.db-shm`.

## Commands

- Main image: `docker build -t ragtech-supervise:local .`
- NUT bridge image: `docker build -f nut-bridge/Dockerfile -t ragtech-nut-bridge:local .`
- Main runtime needs the UPS serial device, typically `--device /dev/ttyACM0:/dev/ttyACM0:rw`.
- NUT bridge runtime requires `-e NUT_MONITOR_PASSWORD=...`; `entrypoint.sh` exits if it is unset.

## Tests

- Preferred fast check: `tests/run-in-container.sh --unit`.
- `--unit` builds `tests/Dockerfile`, then runs `bash -n`, `shellcheck`, and `bats tests/unit`.
- Hardware-free bridge check: `tests/run-in-container.sh --integration`.
- `--integration` requires Docker at `/var/run/docker.sock` and builds/runs containers.
- Run both suites with `tests/run-in-container.sh --all` when Docker socket access is available.
- Host deps installed: `tests/run-tests.sh --unit|--integration|--all` skips the wrapper container.
- Focused Bats: `bats tests/unit/ragtech_to_nut.bats`; `run-tests.sh` adds syntax/ShellCheck gates.
