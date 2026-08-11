#!/bin/sh
set -eu

if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
    echo "usage: $0 ELAN_GUARDIAN_DEB" >&2
    exit 2
fi

package=$1
test "$(dpkg-deb -f "$package" Package)" = elan-guardian
version=$(dpkg-deb -f "$package" Version)
upstream_version=${version%%-*}
test "$upstream_version" = 0.2.13

stage=$(mktemp -d)
cleanup() {
    rm -rf -- "$stage"
}
trap cleanup EXIT HUP INT TERM

dpkg-deb -x "$package" "$stage"
dpkg-deb -e "$package" "$stage/DEBIAN"

binary="$stage/usr/bin/elan-guardian"
scorer="$stage/usr/bin/elan-trace-score"
dkms_root="$stage/usr/src/elan-guardian-$upstream_version"

test -x "$binary"
test -x "$scorer"
test -f "$stage/usr/share/man/man8/elan-guardian.8.gz"
test -f "$stage/usr/lib/systemd/system/elan-guardian-resume.service"
test -f "$stage/usr/lib/systemd/system/elan-guardian-module.service"
test -f "$stage/usr/lib/systemd/system/elan-guardian-watch.service"
test -f "$stage/usr/lib/systemd/system/elan-i2c-recover.service"
test -f "$stage/usr/lib/udev/rules.d/99-elan-i2c-recover.rules"
test -f "$dkms_root/dkms.conf"
test -f "$dkms_root/rust-shim/lib.rs"
test -f "$dkms_root/rust-shim/elan_rs_shim.c"
test -f "$stage/DEBIAN/postinst"
test -f "$stage/DEBIAN/prerm"
test ! -e "$stage/usr/lib/systemd/system/elan-guardian.service"

grep -q '^PACKAGE_VERSION="0.2.13"$' "$dkms_root/dkms.conf"
grep -q 'dkms autoinstall -m elan-guardian -v 0.2.13' "$stage/DEBIAN/postinst"
grep -q 'userspace recovery remains available' "$stage/DEBIAN/postinst"
grep -q 'ExecStart=/usr/bin/elan-guardian watch --affected-only --interval-ms 50' \
    "$stage/usr/lib/systemd/system/elan-guardian-watch.service"

test "$("$binary" --version)" = "elan-guardian $upstream_version"
"$binary" --help | grep -q 'record --output TRACE.json'

fixture="$stage/features.dat"
printf '%s\n' '0 0 1 1' > "$fixture"
test "$("$scorer" "$fixture")" = transport-stalled

readelf -lW "$binary" | grep -q GNU_RELRO
readelf -dW "$binary" | grep -Eq 'BIND_NOW|FLAGS_1.*NOW'
readelf -nW "$binary" | grep -q 'Build ID:'
if readelf -lW "$binary" | grep -q 'GNU_STACK.*RWE'; then
    echo "elan-guardian has an executable stack" >&2
    exit 1
fi
if readelf -dW "$binary" | grep -Eq '\((RPATH|RUNPATH)\)'; then
    echo "elan-guardian contains a runtime library search path" >&2
    exit 1
fi

if command -v udevadm >/dev/null; then
    udevadm verify "$stage/usr/lib/udev/rules.d/99-elan-i2c-recover.rules"
fi
