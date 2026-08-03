#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 || ! -f $1 ]]; then
    printf '%s\n' "usage: $0 ELAN_GUARDIAN_RPM" >&2
    exit 2
fi
package=$1
[[ $(rpm -qp --qf '%{NAME}' "$package") == elan-guardian ]]
version=$(rpm -qp --qf '%{VERSION}' "$package")

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
module_service=$stage/usr/lib/systemd/system/elan-guardian-module.service
watch_service=$stage/usr/lib/systemd/system/elan-guardian-watch.service
recovery_service=$stage/usr/lib/systemd/system/elan-i2c-recover.service
recovery_rule=$stage/usr/lib/udev/rules.d/99-elan-i2c-recover.rules
watch_interval=$stage/usr/lib/systemd/system/elan-guardian-watch.service.d/50-interval.conf

[[ -x $binary && -x $scorer ]]
[[ -f $service && -f $module_service && -f $watch_service ]]
[[ -f $recovery_service && -f $recovery_rule && -f $watch_interval ]]
[[ -f $stage/usr/lib/systemd/system-preset/91-elan-guardian.preset ]]
[[ -f $stage/usr/share/man/man8/elan-guardian.8.gz ]]
[[ -f $stage/usr/share/doc/elan-guardian/formal/agda/ElanGuardian.agda ]]
[[ -f $stage/usr/share/doc/elan-guardian/formal/idris/ElanPolicy.idr ]]
[[ -f $stage/usr/share/doc/elan-guardian/kernel/0001-input-elan-i2c-add-in-place-recovery.patch ]]
[[ -f $stage/usr/src/elan-guardian-$version/rust-shim/lib.rs ]]
[[ -f $stage/usr/src/elan-guardian-$version/rust-shim/elan_rs_shim.c ]]
[[ ! -e $stage/usr/lib/systemd/system/elan-guardian.service ]]
rg -F 'ExecStop=/usr/bin/elan-guardian recover --all --affected-only --quiet' "$service"
rg -F 'WantedBy=sleep.target' "$service"
rg -F 'ConditionPathExists=!/usr/lib/systemd/system/libinput-rs-elan-resume.service' "$service"
rg -F 'ExecStart=/usr/bin/elan-guardian activate-module --affected-only' "$module_service"
rg -F 'CapabilityBoundingSet=CAP_SYS_MODULE' "$module_service"
rg -F 'ExecStart=/usr/bin/elan-guardian watch --affected-only --interval-ms 50' "$watch_service"
rg -F '/usr/bin/elan-guardian watch --affected-only --interval-ms 50' "$watch_interval"
rg -F 'Wants=elan-guardian-module.service' "$watch_service"
rg -F 'WantedBy=multi-user.target' "$watch_service"

[[ $("$binary" --version) == "elan-guardian $version" ]]
"$binary" --help | rg 'record --output TRACE\.json'
"$binary" --help | rg 'activate-module \[--affected-only\]'
"$binary" --help | rg 'watch \[--affected-only\]'

readelf -lW "$binary" | rg 'GNU_RELRO'
readelf -dW "$binary" | rg 'BIND_NOW|FLAGS_1.*NOW'
readelf -nW "$binary" | rg 'Build ID:'
! readelf -lW "$binary" | rg 'GNU_STACK.*RWE'
! readelf -dW "$binary" | rg '\((RPATH|RUNPATH)\)'

fixture=$stage/features.dat
printf '%s\n' '0 0 1 1' >"$fixture"
[[ $("$scorer" "$fixture") == transport-stalled ]]
rg -F 'recover --device 13-0015 --rebind --quiet' "$recovery_service"
rg -F '99-elan-i2c-recover.rules' "$recovery_rule"
