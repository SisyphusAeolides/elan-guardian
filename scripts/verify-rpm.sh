#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 || ! -f $1 ]]; then
    printf '%s\n' "usage: $0 ELAN_GUARDIAN_RPM" >&2
    exit 2
fi
package=$1
[[ $(rpm -qp --qf '%{NAME}' "$package") == elan-guardian ]]

tmp_root=${TMPDIR:-/tmp}
stage=$(mktemp -d "$tmp_root/elan-guardian-rpm.XXXXXX")
cleanup() {
    case $stage in
        "$tmp_root"/elan-guardian-rpm.*) rm -rf -- "$stage" ;;
        *) printf '%s\n' "refusing to remove unexpected path: $stage" >&2 ;;
    esac
}
trap cleanup EXIT

(
    cd "$stage"
    rpm2cpio "$package" | cpio -idm --quiet
)

binary=$stage/usr/bin/elan-guardian
scorer=$stage/usr/bin/elan-trace-score
service=$stage/usr/lib/systemd/system/elan-guardian-resume.service

[[ -x $binary && -x $scorer ]]
[[ -f $service ]]
[[ -f $stage/usr/lib/systemd/system-preset/91-elan-guardian.preset ]]
[[ -f $stage/usr/share/man/man8/elan-guardian.8.gz ]]
[[ -f $stage/usr/share/doc/elan-guardian/formal/agda/ElanGuardian.agda ]]
[[ -f $stage/usr/share/doc/elan-guardian/formal/idris/ElanPolicy.idr ]]
[[ -f $stage/usr/share/doc/elan-guardian/kernel/0001-input-elan-i2c-add-in-place-recovery.patch ]]
[[ ! -e $stage/usr/lib/systemd/system/elan-guardian.service ]]
rg -F 'ExecStop=/usr/bin/elan-guardian recover --all --affected-only --quiet' "$service"
rg -F 'WantedBy=sleep.target' "$service"

"$binary" --version | rg '^elan-guardian 0\.1\.0$'
"$binary" --help | rg 'record --output TRACE\.json'

readelf -lW "$binary" | rg 'GNU_RELRO'
readelf -dW "$binary" | rg 'BIND_NOW|FLAGS_1.*NOW'
readelf -nW "$binary" | rg 'Build ID:'
! readelf -lW "$binary" | rg 'GNU_STACK.*RWE'
! readelf -dW "$binary" | rg '\((RPATH|RUNPATH)\)'

fixture=$stage/features.dat
printf '%s\n' '0 0 1 1' >"$fixture"
[[ $("$scorer" "$fixture") == transport-stalled ]]
