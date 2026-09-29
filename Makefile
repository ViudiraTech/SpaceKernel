SHELL := /bin/sh

# A generated .config is authoritative; the checked-in defaults make a fresh
# clone buildable without visiting menuconfig first.
ifneq ($(wildcard .config),)
include .config
else
include .config-default
endif

ifeq ($(CONFIG_ARCH_AARCH64),y)
CONFIGURED_ARCH := aarch64
else ifeq ($(CONFIG_ARCH_RISCV64),y)
CONFIGURED_ARCH := riscv64
else
CONFIGURED_ARCH := x86_64
endif
ARCH ?= $(CONFIGURED_ARCH)
PROFILE ?= $(if $(filter y,$(CONFIG_PROFILE_RELEASE)),release,debug)
CPUS ?= $(CONFIG_QEMU_CPUS)
MEMORY_MIB ?= $(CONFIG_QEMU_MEMORY_MIB)

ifeq ($(ARCH),x86_64)
TARGET := x86_64-unknown-none
EFI := BOOTX64.EFI
QEMU := qemu-system-x86_64
FIRMWARE := /usr/share/OVMF/OVMF_CODE_4M.fd
MACHINE := q35
else ifeq ($(ARCH),aarch64)
TARGET := aarch64-unknown-none-softfloat
EFI := BOOTAA64.EFI
QEMU := qemu-system-aarch64
FIRMWARE := /usr/share/AAVMF/AAVMF_CODE.fd
MACHINE := virt
QEMU_CPU := -cpu max
else ifeq ($(ARCH),riscv64)
TARGET := riscv64gc-unknown-none-elf
EFI := BOOTRISCV64.EFI
QEMU := qemu-system-riscv64
FIRMWARE := /usr/share/qemu-efi-riscv64/RISCV_VIRT_CODE.fd
MACHINE := virt
else
$(error Unsupported ARCH=$(ARCH))
endif

ifeq ($(PROFILE),release)
CARGO_PROFILE := --release
else ifeq ($(PROFILE),debug)
CARGO_PROFILE :=
else
$(error Unsupported PROFILE=$(PROFILE))
endif

ifeq ($(CONFIG_BOOT_SELF_TEST),y)
CARGO_FEATURES := --features boot-self-test
else
CARGO_FEATURES :=
endif

OUT := build/$(ARCH)/$(PROFILE)
KERNEL := target/$(TARGET)/$(PROFILE)/spacekernel
ISO := $(OUT)/SpaceKernel-$(ARCH).iso
ISO_ROOT := $(OUT)/iso
LIMINE_VERSION := v12.9.1
LIMINE_DIR := build/limine/$(LIMINE_VERSION)
LIMINE_ARCHIVE := build/downloads/limine-$(LIMINE_VERSION)-binary.tar.gz
LIMINE_SHA256 := 5cdebc518daa3af30b22c2322ba0dba2e0e2046fa8b087b9e13071b8dbcdcff4
QEMU_FLAGS := -machine $(MACHINE) $(QEMU_CPU) -m $(MEMORY_MIB) -smp $(CPUS) -serial stdio -monitor none -no-reboot

.PHONY: all kernel iso run debug test check fmt menuconfig defconfig olddefconfig deps limine clean help
all: iso

defconfig:
	kconfig-conf --alldefconfig Kconfig

olddefconfig:
	kconfig-conf --olddefconfig Kconfig

menuconfig:
	kconfig-mconf Kconfig

deps:
	rustup component add rust-src
	rustup target add $(TARGET)
	cargo fetch --target $(TARGET)

kernel: deps
	cargo build --target $(TARGET) $(CARGO_PROFILE) $(CARGO_FEATURES)

$(LIMINE_ARCHIVE):
	mkdir -p build/downloads
	curl -fL --retry 3 -o $@ https://github.com/Limine-Bootloader/Limine/releases/download/$(LIMINE_VERSION)/limine-binary.tar.gz

$(LIMINE_DIR)/.extracted: $(LIMINE_ARCHIVE)
	printf '%s  %s\n' $(LIMINE_SHA256) $(LIMINE_ARCHIVE) | sha256sum -c -
	mkdir -p $(LIMINE_DIR)
	tar -xzf $< -C $(LIMINE_DIR) --strip-components=1
	touch $@

limine: $(LIMINE_DIR)/.extracted
	$(MAKE) -C $(LIMINE_DIR)

iso: kernel limine
	rm -rf $(ISO_ROOT)
	mkdir -p $(ISO_ROOT)/EFI/BOOT $(ISO_ROOT)/boot/limine
	cp $(KERNEL) $(ISO_ROOT)/kernel.elf
	cp $(LIMINE_DIR)/$(EFI) $(ISO_ROOT)/EFI/BOOT/$(EFI)
	cp $(LIMINE_DIR)/limine-bios.sys $(LIMINE_DIR)/limine-bios-cd.bin $(LIMINE_DIR)/limine-uefi-cd.bin $(ISO_ROOT)/boot/limine/
	python3 tools/render_limine.py boot/limine.conf.in $(ISO_ROOT)/limine.conf $(if $(filter y,$(CONFIG_KASLR)),yes,no)
	xorriso -as mkisofs -R -r -J $(if $(filter x86_64,$(ARCH)),-b boot/limine/limine-bios-cd.bin -no-emul-boot -boot-load-size 4 -boot-info-table,) -hfsplus -apm-block-size 2048 --efi-boot boot/limine/limine-uefi-cd.bin -efi-boot-part --efi-boot-image --protective-msdos-label -o $(ISO) $(ISO_ROOT)
	$(if $(filter x86_64,$(ARCH)),$(LIMINE_DIR)/limine bios-install $(ISO),true)
	@printf 'ISO ready: %s\n' '$(ISO)'

run: iso
	$(QEMU) $(QEMU_FLAGS) -display gtk -drive if=pflash,format=raw,readonly=on,file=$(FIRMWARE) -cdrom $(ISO)

debug: iso
	$(QEMU) $(QEMU_FLAGS) -display gtk -S -s -drive if=pflash,format=raw,readonly=on,file=$(FIRMWARE) -cdrom $(ISO)

test: iso
	python3 tools/qemu_test.py $(ARCH) $(ISO) $(FIRMWARE) $(CPUS) $(MEMORY_MIB)

check:
	cargo fmt --all -- --check
	@for target in x86_64-unknown-none aarch64-unknown-none-softfloat riscv64gc-unknown-none-elf; do \
	  rustup target add $$target && cargo check --target $$target --all-features || exit 1; \
	done

fmt:
	cargo fmt --all

clean:
	cargo clean
	rm -rf build

help:
	@printf '%s\n' \
	  'make menuconfig   Configure architecture, profile, KASLR and QEMU' \
	  'make defconfig    Restore checked-in Kconfig defaults' \
	  'make             Build bootable ISO' \
	  'make run         Boot ISO in QEMU' \
	  'make debug       Start QEMU paused with GDB port 1234' \
	  'make test        Boot smoke test' \
	  'make check       Format and check all target architectures'
