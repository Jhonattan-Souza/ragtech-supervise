#!/usr/bin/env bash
set -euo pipefail

pgrep -x supsvc >/dev/null
timeout 3 bash -ec 'true </dev/tcp/127.0.0.1/4470'
