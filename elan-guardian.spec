Name:           elan-guardian
Version:        0.2.14
Release:        1%{?dist}
Summary:        Evidence-driven diagnostics and recovery for Elantech I2C input
License:        GPL-2.0-only AND (Apache-2.0 OR MIT) AND (Unlicense OR MIT) AND Unicode-3.0
URL:            https://github.com/SisyphusAeolides/elan-guardian
Source0:        https://github.com/SisyphusAeolides/elan-guardian/archive/refs/tags/v%{version}.tar.gz

BuildRequires:  cargo >= 1.75
BuildRequires:  rust >= 1.75
BuildRequires:  gcc-gfortran
BuildRequires:  systemd-rpm-macros
Requires:       dkms
Requires:       binutils
Requires:       kmod
Requires:       rust >= 1.75

%description
Elan Guardian records Elantech IRQ and evdev activity to distinguish transport,
kernel driver, and userspace input failures. It can reinitialize only devices
already bound to the elan_i2c driver and never grabs input devices or creates a
virtual pointer. Agda, Idris 2, and Fortran models provide independent checks of
its state and classification policy. A no_std Rust packet, watchdog, and
recovery core plus a C Linux ABI shim can be built as a replacement elan_i2c
module even when the target kernel has CONFIG_RUST disabled.

%prep
%autosetup

%build
%set_build_flags
CARGO_NET_OFFLINE=true cargo build --frozen --release
mkdir -p target/release
gfortran %{build_fflags} %{build_ldflags} -std=f2018 \
    -o target/release/elan-trace-score fortran/elan_trace_score.f90

%install
install -Dm755 target/release/elan-guardian %{buildroot}%{_bindir}/elan-guardian
install -Dm755 target/release/elan-trace-score %{buildroot}%{_bindir}/elan-trace-score
install -Dm644 packaging/elan-guardian.8 %{buildroot}%{_mandir}/man8/elan-guardian.8
install -Dm644 systemd/elan-guardian-resume.service \
    %{buildroot}%{_unitdir}/elan-guardian-resume.service
install -Dm644 systemd/elan-guardian-module.service \
    %{buildroot}%{_unitdir}/elan-guardian-module.service
install -Dm644 systemd/elan-guardian-watch.service \
    %{buildroot}%{_unitdir}/elan-guardian-watch.service
install -Dm644 systemd/elan-guardian-watch.service.d/50-interval.conf \
    %{buildroot}%{_unitdir}/elan-guardian-watch.service.d/50-interval.conf
install -Dm644 systemd/elan-i2c-recover.service \
    %{buildroot}%{_unitdir}/elan-i2c-recover.service
install -Dm644 systemd/99-elan-i2c-recover.rules \
    %{buildroot}%{_udevrulesdir}/99-elan-i2c-recover.rules
install -Dm644 systemd/91-elan-guardian.preset \
    %{buildroot}%{_presetdir}/91-elan-guardian.preset
install -Dm644 formal/agda/ElanGuardian.agda \
    %{buildroot}%{_docdir}/%{name}/formal/agda/ElanGuardian.agda
install -Dm644 formal/idris/ElanPolicy.idr \
    %{buildroot}%{_docdir}/%{name}/formal/idris/ElanPolicy.idr
install -Dm644 kernel/0001-input-elan-i2c-add-in-place-recovery.patch \
    %{buildroot}%{_docdir}/%{name}/kernel/0001-input-elan-i2c-add-in-place-recovery.patch
install -d %{buildroot}%{_prefix}/src/%{name}-%{version}/rust-shim
cp -a dkms.conf %{buildroot}%{_prefix}/src/%{name}-%{version}/
cp -a kernel/rust-shim/Makefile kernel/rust-shim/README.md \
    kernel/rust-shim/UPSTREAM kernel/rust-shim/elan_core.rs \
    kernel/rust-shim/elan_rs_shim.c kernel/rust-shim/elan_rs_shim.h \
    kernel/rust-shim/lib.rs kernel/rust-shim/upstream \
    %{buildroot}%{_prefix}/src/%{name}-%{version}/rust-shim/
install -d %{buildroot}%{_licensedir}/%{name}/third-party
for crate in vendor/*; do
    test -d "$crate" || continue
    destination=%{buildroot}%{_licensedir}/%{name}/third-party/$(basename "$crate")
    for license in "$crate"/LICENSE* "$crate"/COPYING*; do
        test -f "$license" || continue
        install -d "$destination"
        install -m644 "$license" "$destination/"
    done
done

%post
%systemd_post elan-guardian-resume.service
%systemd_post elan-guardian-watch.service
%systemd_post elan-i2c-recover.service
if dkms add -m %{name} -v %{version} --rpm_safe_upgrade &&
   dkms build -m %{name} -v %{version} &&
   dkms install -m %{name} -v %{version} --force; then
    :
else
    echo "elan-guardian: optional DKMS module unavailable; userspace recovery remains enabled" >&2
fi
if [ "$1" -gt 1 ] && systemctl is-active --quiet elan-guardian-watch.service; then
    systemctl try-restart elan-guardian-watch.service || :
fi

%preun
%systemd_preun elan-guardian-resume.service
%systemd_preun elan-guardian-watch.service
%systemd_preun elan-i2c-recover.service
dkms remove -m %{name} -v %{version} --all --rpm_safe_upgrade || true

%postun
%systemd_postun_with_restart elan-guardian-resume.service
%systemd_postun_with_restart elan-guardian-watch.service
%systemd_postun_with_restart elan-i2c-recover.service

%check
CARGO_NET_OFFLINE=true cargo test --frozen --all-targets
scripts/test-fortran.sh target/release/elan-trace-score

%files
%license LICENSE
%license %{_licensedir}/%{name}/third-party
%doc README.md
%doc %{_docdir}/%{name}/formal
%doc %{_docdir}/%{name}/kernel
%{_prefix}/src/%{name}-%{version}
%{_bindir}/elan-guardian
%{_bindir}/elan-trace-score
%{_mandir}/man8/elan-guardian.8*
%{_unitdir}/elan-guardian-resume.service
%{_unitdir}/elan-guardian-module.service
%{_unitdir}/elan-guardian-watch.service
%{_unitdir}/elan-guardian-watch.service.d/50-interval.conf
%{_unitdir}/elan-i2c-recover.service
%{_udevrulesdir}/99-elan-i2c-recover.rules
%{_presetdir}/91-elan-guardian.preset

%changelog
* Mon Aug 10 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.14-1
- Fix dkms.conf version mismatch for release sync

* Mon Aug 10 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.11-1
- Synchronize release cycle with libinput-rs 0.3.4
- No functional changes to the guardian service

* Sat Aug 08 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.10-1
- Clean optional kernel artifacts without requiring host kernel headers

* Sat Aug 08 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.9-1
- Add native Ubuntu packaging and cross-distribution command discovery
- Require the runtime toolchain used by the optional DKMS module
- Document required package and repository signature verification

* Wed Aug 05 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.8-1
- Prevent repeated controller rebind attempts after a failed recovery

* Mon Aug 03 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.7-1
- Lower elan-guardian-watch poll interval to 50ms for AGY stall detection
- Add AGY-specific in-place ELAN recovery and udev-triggered rebind path
- Remove invasive periodic controller probes from the kernel watchdog
- Export successful report counts and recover only after real report failures
- Keep userspace recovery available when an optional DKMS build is unsupported
- Verify the external module against Linux 6.12 and Linux 7.1

* Sat Aug 01 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.5-1
- Activate a newly installed DKMS module before starting the recovery monitor
- Reload only when the running and installed module identities differ
- Match direct module builds to the target kernel's GCC or LLVM toolchain

* Sat Aug 01 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.4-1
- Select the supported Rust jump-table flag for the kernel module compiler

* Fri Jul 31 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.3-1
- Add DKMS support for permanent rust shim recovery module

* Thu Jul 30 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.2-1
- Detect continuously unread ELAN queues on registered libinput descriptors
- Rebind consumer-stalled controllers without reading or grabbing evdev events
- Poll consumer liveness at 100 ms with a 750 ms continuous-backlog gate

* Thu Jul 30 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.1-1
- Rebind automatically when an in-place kernel recovery does not restore input
- Write sysfs control commands atomically instead of splitting the newline
- Add a non-grabbing recovery monitor for affected ThinkPad P53 systems

* Thu Jul 30 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.2.0-1
- Add a no_std Rust data and recovery core for the elan_i2c kernel module
- Keep Linux I2C, input, IRQ, power, and registration calls behind a C shim
- Build without requiring CONFIG_RUST in the target kernel
- Decode touchpad and TrackPoint reports in Rust
- Enable in-place automatic recovery on affected ThinkPad P53 systems

* Thu Jul 30 2026 Kenny Glowner <SisyphusAeolides@pm.me> - 0.1.0-1
- Record IRQ and evdev evidence without grabbing input devices
- Separate transport, driver, and consumer-side input stalls
- Recover dynamically discovered elan_i2c controllers with bounded retries
- Prefer an in-place recovery interface when supplied by the kernel
- Verify lifecycle and classification policy with Agda, Idris 2, and Fortran
- Run affected-machine recovery after resume without a resident daemon
