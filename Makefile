# Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
# SPDX-License-Identifier: BSD-3-Clause-Clear
#
# Makefile for building minivm and running its tests on QEMU.

CLANG     ?= clang
OBJCOPY   ?= llvm-objcopy
QEMU      ?= qemu-system-hexagon
PYTHON    ?= python3
MINIVM    ?= target/hexagon-unknown-none-elf/release/minivm
BUILD_DIR := target/guest-tests

CARGO_BUILD := cargo build -Zbuild-std=core,alloc \
	-Zbuild-std-features=compiler-builtins-mem

# Allow the Rust linker to be overridden via CC (used in CI with a cross clang).
# When CC is not set, .cargo/config.toml supplies the linker.
ifneq ($(origin CC),default)
export CARGO_TARGET_HEXAGON_UNKNOWN_NONE_ELF_LINKER := $(CC)
endif

GUEST_TESTS := first test_vmversion test_interrupts test_processors test_mmu
# C guests that check the HVM interface operation by operation.
HVM_TESTS := hvm_funcs hvm_intop hvm_smp hvm_mmu hvm_varadix hvm_timer
HVM_RUNTIME := tests/hvm/crt.S tests/hvm/lib.c
# Guests bring their own startup code and linker script; newer CodeLinaro
# toolchains ship a hexagon.cfg that would add picolibc's.
GUEST_CFLAGS := --no-default-config --target=hexagon -mcpu=hexagonv73 -nostdlib \
	-fuse-ld=lld -T tests/guest.ld

.PHONY: minivm minivm-debug guest-tests sim-guest-tests linux-boot \
	clean-guest-tests

minivm:
	$(CARGO_BUILD) --release

minivm-debug:
	$(CARGO_BUILD)

guest-tests: $(addprefix $(BUILD_DIR)/, $(addsuffix .pass, $(GUEST_TESTS) $(HVM_TESTS)))
	@echo "All guest tests passed."

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(BUILD_DIR)/%.elf: tests/%.S hexagon_vm.h tests/guest.ld | $(BUILD_DIR)
	$(CLANG) $(GUEST_CFLAGS) -I. -o $@ $<

$(BUILD_DIR)/%.elf: tests/hvm/%.c $(HVM_RUNTIME) tests/hvm/hvm.h tests/guest.ld | $(BUILD_DIR)
	$(CLANG) $(GUEST_CFLAGS) -ffreestanding -fno-builtin -O1 -G0 \
		-o $@ $(HVM_RUNTIME) $<

$(BUILD_DIR)/%.bin: $(BUILD_DIR)/%.elf
	$(OBJCOPY) -O binary $< $@

.PRECIOUS: $(BUILD_DIR)/%.elf $(BUILD_DIR)/%.bin $(BUILD_DIR)/%.sim.elf

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

# The same guest tests on hexagon-sim, the reference simulator. It loads one
# ELF, so each guest is added to a copy of minivm as an extra segment. The
# simulator's semihosting reads memory through the MMU only with
# --v2p_translation. HEXAGON_TOOLS is the Tools directory of a Hexagon SDK
# toolchain.
HEXAGON_TOOLS ?= /opt/Hexagon_SDK/6.4.0.2/tools/HEXAGON_Tools/19.0.04/Tools
HEXAGON_SIM   ?= $(HEXAGON_TOOLS)/bin/hexagon-sim
SIM_FLAGS     := -mv68 --simulated_returnval --v2p_translation 1 \
	--subsystem_base 0xfe28 --cosim_file $(BUILD_DIR)/q6ss.cfg
# Not test_mmu: it prints PASS or FAIL from user mode, out of a page the
# simulator's semihosting cannot translate, so nothing shows which it was.
SIM_GUEST_TESTS := first test_vmversion test_interrupts test_processors \
	$(HVM_TESTS)

sim-guest-tests: $(addprefix $(BUILD_DIR)/, $(addsuffix .simpass, $(SIM_GUEST_TESTS)))
	@echo "All guest tests passed on hexagon-sim."

$(BUILD_DIR)/q6ss.cfg: | $(BUILD_DIR)
	printf '%s\n' \
		"$(HEXAGON_TOOLS)/lib/iss/qtimer.so --csr_base=0xfe280000 --irq_p=2 --freq=19200000 --cnttid=0x01" \
		"$(HEXAGON_TOOLS)/lib/iss/l2vic.so 32 0xfe290000" > $@

$(BUILD_DIR)/%.sim.elf: $(BUILD_DIR)/%.bin $(MINIVM)
	$(PYTHON) scripts/add_segment.py $(MINIVM) $< 0xa0000000 $@

$(BUILD_DIR)/%.simpass: $(BUILD_DIR)/%.sim.elf $(BUILD_DIR)/q6ss.cfg
	@echo "  SIM  $*"
	@timeout 900 script -qfec "$(HEXAGON_SIM) $(SIM_FLAGS) $<" /dev/null \
		< /dev/null > $(BUILD_DIR)/$*.sim.log 2>&1; rc=$$?; \
	if [ $$rc = 0 ] && grep -q "^PASS" $(BUILD_DIR)/$*.sim.log; then \
		echo "  PASS $*"; \
		touch $@; \
	else \
		echo "  FAIL $* (status $$rc, see $(BUILD_DIR)/$*.sim.log)"; \
		exit 1; \
	fi

# Boot Linux on the QCS6490 CDSP machine with minivm as firmware, log in on
# the console, check the CPU count and power off. KERNEL and INITRD are the
# `vmlinux` and `initrd` of the Hexagon Linux howto.
linux-boot: $(MINIVM)
	@test -n "$(KERNEL)" -a -n "$(INITRD)" || \
		{ echo "usage: make linux-boot KERNEL=vmlinux INITRD=initrd"; exit 2; }
	$(PYTHON) tests/linux_boot.py --qemu $(QEMU) --minivm $(MINIVM) \
		--kernel $(KERNEL) --initrd $(INITRD) $(LINUX_BOOT_ARGS)

clean-guest-tests:
	rm -rf $(BUILD_DIR)
