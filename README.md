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

The bridge maps the latest `EVENTLOG` row as follows:

| NUT variable | Supervise column |
| --- | --- |
| `ups.status` | `flag_opBattery`, `flag_noVInput`, `flag_loBattery`, `fail_endBattery` |
| `battery.charge` | `var_cBattery` |
| `battery.voltage` | `var_vBattery` |
| `input.voltage` | `var_vInput` |
| `output.voltage` | `var_vOutput` |
| `output.current` | `var_iOutput` |
| `output.frequency` | `var_fOutput` |
| `ups.load` | derived from `var_pOutput`, `var_nominalPOutput`, `var_vOutput`, and `var_iOutput` |
| `ups.power.nominal` | `var_nominalPOutput` |
| `ups.alarm` | emitted with `ALARM` directives from warning/fault flags |

When Supervise reports the UPS as disconnected or the database has no current sample, the bridge
publishes `OFF` with an alarm and does not publish the NUT `LB` low-battery flag.

## License

Apache 2.0
