# Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
# SPDX-License-Identifier: BSD-3-Clause-Clear
#
# Makefile for building minivm and running its tests on QEMU.

CLANG     ?= clang
OBJCOPY   ?= llvm-objcopy
QEMU      ?= qemu-system-hexagon
MINIVM    ?= target/hexagon-unknown-none-elf/release/minivm
BUILD_DIR := target/guest-tests

CARGO_BUILD := cargo build -Zbuild-std=core,alloc \
	-Zbuild-std-features=compiler-builtins-mem

# Allow the Rust linker to be overridden via CC (used in CI with a cross clang).
# When CC is not set, .cargo/config.toml supplies the linker.
ifneq ($(origin CC),default)
export CARGO_TARGET_HEXAGON_UNKNOWN_NONE_ELF_LINKER := $(CC)
endif

GUEST_TESTS := first test_vmversion test_interrupts test_mmu
# Guests bring their own startup code and linker script; newer CodeLinaro
# toolchains ship a hexagon.cfg that would add picolibc's.
GUEST_CFLAGS := --no-default-config --target=hexagon -mcpu=hexagonv73 -nostdlib \
	-fuse-ld=lld -T tests/guest.ld

.PHONY: minivm minivm-debug guest-tests clean-guest-tests

minivm:
	$(CARGO_BUILD) --release

minivm-debug:
	$(CARGO_BUILD)

guest-tests: $(addprefix $(BUILD_DIR)/, $(addsuffix .pass, $(GUEST_TESTS)))
	@echo "All guest tests passed."

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(BUILD_DIR)/%.elf: tests/%.S hexagon_vm.h tests/guest.ld | $(BUILD_DIR)
	$(CLANG) $(GUEST_CFLAGS) -I. -o $@ $<

$(BUILD_DIR)/%.bin: $(BUILD_DIR)/%.elf
	$(OBJCOPY) -O binary $< $@

.PRECIOUS: $(BUILD_DIR)/%.elf $(BUILD_DIR)/%.bin

# Run a single test: minivm as the machine's kernel, the guest loaded where
# minivm looks for one when there is no device tree to say otherwise.
# `-bios none` keeps QEMU from loading its bundled firmware. `timeout` runs
# QEMU in a background process group, where taking over a terminal on stdin
# would stop it, so stdin is /dev/null.
$(BUILD_DIR)/%.pass: $(BUILD_DIR)/%.bin $(MINIVM)
	@echo "  TEST $*"
	@timeout 120 $(QEMU) -M virt -bios none -nographic \
		-kernel $(MINIVM) \
		-device "loader,addr=0xa0000000,file=$<" \
		< /dev/null > $(BUILD_DIR)/$*.log 2>&1; rc=$$?; \
	if [ $$rc = 0 ] && grep -q "^PASS" $(BUILD_DIR)/$*.log; then \
		echo "  PASS $*"; \
		touch $@; \
	else \
		echo "  FAIL $* (status $$rc; see $(BUILD_DIR)/$*.log)"; \
		exit 1; \
	fi

clean-guest-tests:
	rm -rf $(BUILD_DIR)
