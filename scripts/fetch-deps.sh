#!/bin/sh
# Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
# SPDX-License-Identifier: BSD-3-Clause-Clear
#
# Download what the tests need, prebuilt, into DIR (default .deps):
#
#   - the CodeLinaro Hexagon toolchain, which has the cross linker the
#     minivm build uses, the clang that builds the guest tests, and the
#     qemu-system-hexagon they run on (built from qemu's hex-next tag);
#   - the Linux kernel and initramfs of the Hexagon Linux howto, for
#     `make linux-boot` (Linux 7.3.0-rc4, bcain-qcs6490-sept27-2026).
#
# Every file is checked against a pinned SHA-256. Files already present
# and correct are not downloaded again, so DIR can be cached.

set -eu

DIR=${1:-.deps}
BASE=https://artifacts.codelinaro.org/artifactory/codelinaro-toolchain-for-hexagon/23.1.2
TOOLCHAIN=clang+llvm-23.1.2-cross-hexagon-unknown-linux-musl

mkdir -p "$DIR"
cd "$DIR"

# fetch URL FILE SHA256
fetch() {
	if [ -f "$2" ] && echo "$3  $2" | sha256sum -c --status -; then
		return
	fi
	echo "fetching $2"
	curl -fsSL --retry 3 -o "$2.part" "$1"
	echo "$3  $2.part" | sha256sum -c --quiet -
	mv "$2.part" "$2"
}

fetch "$BASE/$TOOLCHAIN.tar.zst" "$TOOLCHAIN.tar.zst" \
	a37d95a6c19232640d1bdc954728813502706c5e4e4f968986ebe4f157b6a432
fetch "$BASE/qcs6490-cdsp/vmlinux" vmlinux \
	1908e9c684df4eb44d700c4096e1b2f73af6652a874e377cd8c6af0f5760d66a
fetch "$BASE/qcs6490-cdsp/initrd" initrd \
	2b958945b38db54145051f76b886f42dd5dee70db399f6222ccc54808d6bdd6b

# Unpack the toolchain once per tarball.
if [ ! -f "$TOOLCHAIN/.unpacked" ] || [ -n "$(find "$TOOLCHAIN.tar.zst" -newer "$TOOLCHAIN/.unpacked")" ]; then
	echo "unpacking $TOOLCHAIN"
	rm -rf "$TOOLCHAIN"
	zstd -dc "$TOOLCHAIN.tar.zst" | tar -x
	touch "$TOOLCHAIN/.unpacked"
fi
