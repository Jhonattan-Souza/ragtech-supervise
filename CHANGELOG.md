# Changelog

All notable changes to this project are documented in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed

- The NUT bridge exporter (`ragtech-to-nut`) is rewritten in Rust (`nut-bridge/`, crate
  `ragtech-nut-bridge`) and built inside the bridge image for the target platform, so the image
  builds natively on both `linux/amd64` and `linux/arm64`. Environment variables, modes
  (`--once`, `--wait-for-valid`), the state file layout and exit status 75 are unchanged.
- Text from the Supervise database (`device.model`, `device.serial`, `ups.firmware`) is written
  as a quoted value, so `#`, quotes and backslashes reach NUT literally instead of truncating or
  breaking the line.
- A missing battery charge is now removed with the explicit empty token; a bare
  `battery.charge:` line made `dummy-ups` reuse the previous line's value.
- Staleness (`MAX_SAMPLE_AGE`) is measured with a monotonic clock, so wall-clock jumps (NTP
  corrections on boards without an RTC) no longer mark a live sample stale. It is compared with
  sub-second precision instead of whole seconds, so a sample can go stale up to one second sooner.
- Measurements of 1e15 or more are removed like non-numeric values, as the shell exporter did,
  instead of being written as numbers too long for dummy-ups' 256-byte value buffer.
- Numeric settings are read as decimal: `BATTERY_CHARGE_LOW=020` is 20 (the shell read it as octal
  16) and `MAX_SAMPLE_AGE=00` disables the age check like `0`.
- The exporter logs each telemetry state change once, with the underlying error, instead of
  logging every failed poll.
- Unknown command-line arguments exit with status 64 instead of starting the polling loop.
- The bridge image no longer installs the `sqlite3` package; SQLite is compiled into the exporter.

### Fixed

- `MAX_SAMPLE_AGE` now defaults to 120 seconds instead of 30. Supervise 8.9 commits samples in
  batches about every 40 seconds, so with 30 the bridge declared live telemetry stale between
  batches and stopped the container roughly once a minute.

### Removed

- `nut-bridge/ragtech-to-nut.sh` and its Bats unit tests, replaced by the Rust crate and its
  test suite (`cargo test --workspace`).
