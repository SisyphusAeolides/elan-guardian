#!/usr/bin/env bash
set -euo pipefail

binary=${1:-target/release/elan-trace-score}
tmp_root=${TMPDIR:-/tmp}
fixture=$(mktemp "$tmp_root/elan-guardian-features.XXXXXX")
cleanup() {
    rm -f -- "$fixture"
}
trap cleanup EXIT

printf '%s\n' '0 0 1 1' >"$fixture"
test "$("$binary" "$fixture")" = transport-stalled
printf '%s\n' '4 0 1 1' >"$fixture"
test "$("$binary" "$fixture")" = driver-stalled
printf '%s\n' '4 24 1 1' >"$fixture"
test "$("$binary" "$fixture")" = consumer-stalled
printf '%s\n' '4 24 1 0' >"$fixture"
test "$("$binary" "$fixture")" = healthy
printf '%s\n' '0 0 3' >"$fixture"
test "$("$binary" --watchdog "$fixture")" = disarmed
printf '%s\n' '1 1 0' >"$fixture"
test "$("$binary" --watchdog "$fixture")" = observe
printf '%s\n' '1 0 0' >"$fixture"
test "$("$binary" --watchdog "$fixture")" = recover-in-place
printf '%s\n' '1 1 3' >"$fixture"
test "$("$binary" --watchdog "$fixture")" = recover-in-place
