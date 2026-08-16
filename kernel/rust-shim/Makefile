ifneq ($(KERNELRELEASE),)

obj-m += elan_i2c.o
elan_i2c-y := \
	upstream/elan_i2c_core.o \
	upstream/elan_i2c_i2c.o \
	upstream/elan_i2c_smbus.o \
	elan_rs_shim.o \
	elan_rs_core.o
ccflags-y += -I$(src)

else

KDIR ?= /lib/modules/$(shell uname -r)/build
RUSTC ?= rustc
OBJCOPY ?= objcopy
OBJDUMP ?= objdump
NM ?= nm
KCONFIG := $(KDIR)/.config
KERNEL_USES_CLANG := $(shell grep -c '^CONFIG_CC_IS_CLANG=y' $(KCONFIG) 2>/dev/null)
FUNCTION_PADDING := $(shell sed -n 's/^CONFIG_FUNCTION_PADDING_BYTES=//p' $(KCONFIG) 2>/dev/null)
RUSTC_SUPPORTS_C_JUMP_TABLES := $(shell RUSTC_BOOTSTRAP=1 $(RUSTC) -C help 2>/dev/null | grep -q 'jump-tables=val' && echo y)
RUSTFLAGS := \
	--edition=2021 \
	--crate-name elan_rs_core \
	--crate-type=lib \
	--emit=obj \
	-C opt-level=2 \
	-C panic=abort \
	-C embed-bitcode=no \
	-C lto=no \
	-C codegen-units=1 \
	-C symbol-mangling-version=v0 \
	-C code-model=kernel \
	-C relocation-model=static \
	-C no-redzone=yes \
	-C force-frame-pointers=yes \
	-C force-unwind-tables=no \
	-C overflow-checks=no \
	-C debuginfo=0 \
	-Z function-sections=no

ifeq ($(origin LLVM),undefined)
ifeq ($(KERNEL_USES_CLANG),1)
KBUILD_TOOLCHAIN := LLVM=1
endif
else
KBUILD_TOOLCHAIN := LLVM=$(LLVM)
endif

ifeq ($(shell grep -c '^CONFIG_MITIGATION_RETHUNK=y' $(KCONFIG) 2>/dev/null),1)
RUSTFLAGS += -Z function-return=thunk-extern
endif
ifeq ($(shell grep -c '^CONFIG_X86_KERNEL_IBT=y' $(KCONFIG) 2>/dev/null),1)
RUSTFLAGS += -Z cf-protection=branch
ifeq ($(RUSTC_SUPPORTS_C_JUMP_TABLES),y)
RUSTFLAGS += -Cjump-tables=n
else
RUSTFLAGS += -Z no-jump-tables
endif
endif
ifneq ($(FUNCTION_PADDING),)
RUSTFLAGS += -Z patchable-function-entry=$(FUNCTION_PADDING),$(FUNCTION_PADDING)
endif

.PHONY: all clean inspect

all: elan_rs_core.o
	$(MAKE) -C $(KDIR) M=$(CURDIR) $(KBUILD_TOOLCHAIN) modules

elan_rs_core.o: lib.rs elan_core.rs
	RUSTC_BOOTSTRAP=1 $(RUSTC) $(RUSTFLAGS) -o $@ lib.rs
	$(OBJCOPY) --remove-section=.eh_frame --remove-section=.rela.eh_frame $@
	@if $(OBJDUMP) -d $@ | grep -Eq '%(xmm|ymm|zmm|mm[0-7])'; then \
		echo "Rust core contains forbidden SIMD/FPU instructions" >&2; exit 1; \
	fi
	@if $(OBJDUMP) -d $@ | grep -Eq '\<(call|jmp)[^#\n]*\*'; then \
		echo "Rust core contains an unmitigated indirect branch" >&2; exit 1; \
	fi
	@if $(NM) -u $@ | grep -Evq '^[[:space:]]+U __x86_return_thunk$$'; then \
		echo "Rust core has unexpected external symbols:" >&2; $(NM) -u $@; exit 1; \
	fi
	touch .elan_rs_core.o.cmd

inspect: all
	modinfo ./elan_i2c.ko
	nm -u ./elan_i2c.ko

clean:
	@if [ -d "$(KDIR)" ]; then \
		$(MAKE) -C "$(KDIR)" M="$(CURDIR)" $(KBUILD_TOOLCHAIN) clean; \
	else \
		rm -f -- *.ko *.mod *.mod.c *.o .*.cmd Module.symvers modules.order; \
	fi
	rm -f elan_rs_core.o

endif
