# Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
# SPDX-License-Identifier: BSD-3-Clause-Clear
#
# Makefile for building and running guest test binaries on QEMU.

CLANG     ?= clang
OBJCOPY   ?= llvm-objcopy
QEMU      ?= qemu-system-hexagon
MINIVM    ?= target/hexagon-unknown-none-elf/debug/minivm
BUILD_DIR := target/guest-tests

# Allow the Rust linker to be overridden via CC (used in CI with a cross clang).
# When CC is not set, .cargo/config.toml supplies the linker.
ifdef CC
export CARGO_TARGET_HEXAGON_UNKNOWN_NONE_ELF_LINKER := $(CC)
endif

GUEST_TESTS := first test_vmversion test_interrupts test_processors test_mmu

.PHONY: guest-tests minivm minivm-with-tests on-target-tests zephyr-boot clean-guest-tests

guest-tests: $(addprefix $(BUILD_DIR)/, $(addsuffix .pass, $(GUEST_TESTS)))
	@echo "All guest tests passed."

minivm:
	cargo build -Zbuild-std=core,alloc -Zbuild-std-features=compiler-builtins-mem

minivm-with-tests:
	cargo build -Zbuild-std=core,alloc \
		-Zbuild-std-features=compiler-builtins-mem --features run-tests

on-target-tests: minivm-with-tests
	@echo "  ON-TARGET TESTS"
	timeout 60 $(QEMU) -M virt -nographic \
		-kernel target/hexagon-unknown-none-elf/debug/minivm

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(BUILD_DIR)/%.elf: tests/%.S hexagon_vm.h tests/guest.ld | $(BUILD_DIR)
	$(CLANG) --target=hexagon -mcpu=hexagonv73 -nostdlib -fuse-ld=lld \
		-T tests/guest.ld -I. -o $@ $<

$(BUILD_DIR)/%.bin: $(BUILD_DIR)/%.elf
	$(OBJCOPY) -O binary $< $@

# Run a single test: load guest binary at 0xa0000000 (boot_guest entry),
# then check output for PASS.
$(BUILD_DIR)/%.pass: $(BUILD_DIR)/%.bin $(MINIVM)
	@echo "  TEST $*"
	@timeout 30 $(QEMU) -M virt -nographic \
		-kernel $(MINIVM) \
		-device "loader,addr=0xa0000000,file=$<" \
		> $(BUILD_DIR)/$*.log 2>&1; \
	if grep -q "PASS" $(BUILD_DIR)/$*.log; then \
		echo "  PASS $*"; \
		touch $@; \
	else \
		echo "  FAIL $* (see $(BUILD_DIR)/$*.log)"; \
		exit 1; \
	fi

ZEPHYR_BIN ?= tests_bin/zephyr.bin

zephyr-boot: minivm
	timeout 30 $(QEMU) -M virt -nographic -m 4G \
		-kernel $(MINIVM) \
		-device "loader,addr=0xa0000000,file=$(ZEPHYR_BIN)"

clean-guest-tests:
	rm -rf $(BUILD_DIR)
