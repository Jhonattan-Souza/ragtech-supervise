# Ragtech Supervise Docker Image

Ragtech is a Brazilian company that produces UPS (more commonly known in Brazil as no-break)
devices. They have a software called Supervise that is used to monitor and control the UPS devices.

The existing Supervise software packaging is not really suitable with modern Linux distributions.
This project aims to provide a Docker image with Supervise supplying all required dependencies.

## Usage

You can run a new container from the computer where the UPS USB cable is plugged to. Validate that
the serial interface created by this USB is named `/dev/ttyACM0` and replace it accordingly:

```
$ docker run -d --name supervise --device /dev/ttyACM0:rw -p 4470:4470 ghcr.io/kriansa/ragtech-supervise:latest
```

## Logging

All log output is written to stdout and stderr. Logs are categorized with 5 different prefixes:
  - `init`: Logs related to the container initialization/termination
  - `main`: Logs referring to the stderr/stdout of the main `supsvc` process
  - `supsvc`: Logs related to the main supervise functionality
  - `device-manager`: Unknown, but I assume it's related to the communication with the UPS'es
  - `serialhid`: Logs related to the Serial interface to the UPS

## Interface

If you want to access the web interface, head to `http://localhost:4470` in your browser.

Alternatively, you can have programatic access to the UPS data connecting to the underlying SQLite
database used to store the logged information. It's really useful if you want, for instance, to
create a metrics exporter out of the UPS data. 

**IMPORTANT:** The SQLite database is set to use `WAL` as the journaling mode, so you can read the
database while it's being written to. Because of that, you need to also account for all the database
files:
  - /data/monit.db
  - /data/monit.db-wal
  - /data/monit.db-shm

This is how you would run the container with the database mounted to the host filesystem:

```
$ mkdir host-db-path
$ docker run [...] -v ./host-db-path:/data ghcr.io/kriansa/ragtech-supervise:latest
```

## NUT bridge

This repository also includes an optional NUT bridge container. It reads the same Supervise SQLite
database and exposes a virtual NUT UPS named `ragtech` using the `dummy-ups` driver in
`dummy-loop` mode, so the running NUT driver keeps rereading the generated state file.

Run the Supervise container with `/data` mounted on the host:

```
$ mkdir -p ./ragtech-data
$ docker run -d --name ragtech-supervise \
    --restart unless-stopped \
    --device /dev/ttyACM0:/dev/ttyACM0:rw \
    -p 4470:4470 \
    -v ./ragtech-data:/data \
    ghcr.io/kriansa/ragtech-supervise:latest
```

Build and run the bridge:

```
$ docker build -f nut-bridge/Dockerfile -t ragtech-nut-bridge:local .
$ docker run -d --name ragtech-nut-bridge \
    --restart unless-stopped \
    -p 3493:3493 \
    -v ./ragtech-data:/data \
    -e NUT_MONITOR_USER=monuser \
    -e NUT_MONITOR_PASSWORD='replace-with-a-strong-password' \
    ragtech-nut-bridge:local
```

`NUT_MONITOR_PASSWORD` is required. The bridge refuses to start without an explicit password because
the user has `upsmon primary` rights.

By default, the bridge treats the first SQLite sample observed after process startup as a baseline
and does not expose it as live telemetry. This prevents an old `OB LB` row from being replayed just
because the SQLite files were touched after the container starts. Set `REQUIRE_FRESH_SAMPLE=0` only
if you intentionally want to expose the last persisted sample before Supervise writes a new one.

After a sample has been accepted, the bridge also requires the SQLite source row to change within
`MAX_SAMPLE_AGE` seconds, defaulting to `30`. When the database is unreadable, has no current row,
or the current row is stale, the bridge reports `ups.status=ALARM` and
`experimental.ragtech.sample.valid=0` instead of refreshing old measurements as live telemetry. Set
`MAX_SAMPLE_AGE=0` only if your Supervise database is expected to keep the same latest row for long
periods.

The bridge healthcheck requires `experimental.ragtech.sample.valid=1` by default, so a missing,
unreadable, or stale database fails health after Docker's startup grace. Set
`HEALTHCHECK_REQUIRE_VALID_SAMPLE=0` if you need the container healthcheck to ignore telemetry
validity and only verify the NUT service itself.

The `/data` mount is intentionally read-write. The bridge only reads Supervise data, but SQLite WAL
readers may need to update lock/shared-memory state while reading the live database.

If the host already has a native NUT service listening on `3493`, publish the bridge on another
host port while testing:

```
$ docker run -d --name ragtech-nut-bridge \
    --restart unless-stopped \
    -p 3494:3493 \
    -v ./ragtech-data:/data \
    -e NUT_MONITOR_USER=monuser \
    -e NUT_MONITOR_PASSWORD='replace-with-a-strong-password' \
    ragtech-nut-bridge:local
```

If the Supervise container already owns the `/data` volume and you do not know its host path, you
can reuse it directly:

```
$ docker run -d --name ragtech-nut-bridge \
    --restart unless-stopped \
    --volumes-from ragtech-supervise \
    -p 3494:3493 \
    -e NUT_MONITOR_USER=monuser \
    -e NUT_MONITOR_PASSWORD='replace-with-a-strong-password' \
    ragtech-nut-bridge:local
```

Validate from any machine with NUT client tools:

```
$ upsc ragtech@localhost
```

Or, when published on host port `3494`:

```
$ upsc ragtech@localhost:3494
```

The bridge maps the latest Supervise sample as follows:

| NUT variable | Supervise column |
| --- | --- |
| `ups.status` | `flag_connected`, `flag_opBattery`, `flag_noVInput`, `flag_loBattery`, `fail_endBattery`, `flag_hiPOutput`, `fail_overload`, `flag_noBattery`, `var_cBattery` |
| `battery.charge` | `var_cBattery` |
| `battery.voltage` | `var_vBattery` |
| `input.voltage` | `var_vInput` |
| `output.voltage` | `var_vOutput` |
| `output.current` | `var_iOutput` |
| `output.frequency` | `var_fOutput` |
| `ups.load` | derived from `var_pOutput`, `var_nominalPOutput`, `var_vOutput`, and `var_iOutput` |
| `ups.power.nominal` | `var_nominalPOutput` |
| `ups.alarm` | emitted with `ALARM` directives from warning/fault flags |

When Supervise reports the UPS as disconnected, the bridge keeps the SQLite sample valid but
publishes `ups.status=ALARM` with `experimental.ragtech.connection.status=disconnected`. Standard
NUT clients will no longer see disconnected or unavailable telemetry as `OL`.

## Tests

Run the fast local validation suite in a containerized Debian test environment:

```
$ tests/run-in-container.sh --unit
```

Run the hardware-free NUT bridge integration tests when Docker daemon access is available:

```
$ tests/run-in-container.sh --integration
```

Both modes run the shell syntax and ShellCheck gates first:

```
$ bash -n init.sh healthcheck.sh nut-bridge/*.sh
$ shellcheck init.sh healthcheck.sh nut-bridge/*.sh
```

## License

Apache 2.0
