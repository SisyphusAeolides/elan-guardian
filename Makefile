NAME := elan-guardian
VERSION := 0.2.4
RPM_TOPDIR ?= $(HOME)/rpmbuild

.PHONY: all rust fortran kmod check formal-check clean dist srpm packaging-check

all: rust fortran

rust:
	cargo build --locked --release

fortran:
	mkdir -p target/release
	gfortran -std=f2018 -O2 -Wall -Wextra -Werror \
		-o target/release/elan-trace-score fortran/elan_trace_score.f90

kmod:
	$(MAKE) -C kernel/rust-shim

check: all
	cargo fmt --all -- --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo test --locked --all-targets
	scripts/test-fortran.sh target/release/elan-trace-score
	$(MAKE) formal-check

formal-check:
	@if command -v agda >/dev/null; then agda -i formal/agda formal/agda/ElanGuardian.agda; else echo "Agda unavailable; skipped"; fi
	@if command -v idris2 >/dev/null; then idris2 --source-dir formal/idris --check formal/idris/ElanPolicy.idr; else echo "Idris 2 unavailable; skipped"; fi

packaging-check:
	test -f elan-guardian.spec
	test -f systemd/elan-guardian-resume.service
	test -f systemd/elan-guardian-watch.service
	test -f packaging/elan-guardian.8
	grep -q '^Version:[[:space:]]*$(VERSION)$$' elan-guardian.spec
	grep -q 'ExecStop=/usr/bin/elan-guardian recover --all --affected-only --quiet' systemd/elan-guardian-resume.service
	grep -q 'ExecStart=/usr/bin/elan-guardian watch --affected-only --interval-ms 100' systemd/elan-guardian-watch.service

dist:
	mkdir -p target/dist
	git archive --format=tar.gz --prefix=$(NAME)-$(VERSION)/ \
		-o target/dist/$(NAME)-$(VERSION).tar.gz HEAD

srpm: dist
	mkdir -p $(RPM_TOPDIR)/SOURCES $(RPM_TOPDIR)/SRPMS
	cp target/dist/$(NAME)-$(VERSION).tar.gz \
		$(RPM_TOPDIR)/SOURCES/v$(VERSION).tar.gz
	rpmbuild -bs elan-guardian.spec --define "_topdir $(RPM_TOPDIR)"

clean:
	cargo clean
	$(MAKE) -C kernel/rust-shim clean
	rm -f *.mod
	find formal -type f -name '*.agdai' -delete
