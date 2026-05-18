# Agent Notes

## Shape

- This is a Docker/shell repo, not an app with a package manager; there are no root manifests, lockfiles, or unit-test configs.
- The root `Dockerfile` builds the main Ragtech Supervise image and downloads `supervise-${SUPERVISE_VERSION}.sh` during build.
- `nut-bridge/` is a separate optional Docker image that reads the Supervise SQLite DB and exposes NUT on port `3493`.

## Commands

- Main image: `docker build -t ragtech-supervise:local .`
- NUT bridge image: `docker build -f nut-bridge/Dockerfile -t ragtech-nut-bridge:local .`
- Fast local script check: `bash -n init.sh healthcheck.sh nut-bridge/*.sh`
- Main container runtime needs the UPS serial device, typically `--device /dev/ttyACM0:/dev/ttyACM0:rw`, and exposes the web UI on `4470`.
- NUT bridge runtime requires `-e NUT_MONITOR_PASSWORD=...`; `entrypoint.sh` exits if it is unset.
