# SPDX-License-Identifier: MPL-2.0
export PATH := /opt/riscv/bin:$(PATH)

BINARY_CACHE_DIR := /opt/linux_binary_cache
VDSO_DIR := ../target
VDSO_LIB := $(VDSO_DIR)/vdso64.so
MKFILE_PATH := $(abspath $(lastword $(MAKEFILE_LIST)))
CUR_DIR := $(patsubst %/,%,$(dir $(MKFILE_PATH)))
ATOMIC_WGET := $(CUR_DIR)/../tools/atomic_wget.sh
BUILD_DIR := $(CUR_DIR)/build
INITRAMFS := $(BUILD_DIR)/initramfs
INITRAMFS_FILELIST := $(BUILD_DIR)/initramfs.filelist
INITRAMFS_IMAGE := $(BUILD_DIR)/initramfs.cpio.gz
EXT2_IMAGE := $(BUILD_DIR)/ext2.img
EXFAT_IMAGE := $(BUILD_DIR)/exfat.img
INITRAMFS_EMPTY_DIRS := \
	$(INITRAMFS)/root \
	$(INITRAMFS)/tmp \
	$(INITRAMFS)/opt \
	$(INITRAMFS)/proc \
	$(INITRAMFS)/dev \
	$(INITRAMFS)/ext2 \
	$(INITRAMFS)/exfat
INITRAMFS_ALL_DIRS := \
	$(INITRAMFS)/etc \
	$(INITRAMFS)/lib/x86_64-linux-gnu \
	$(INITRAMFS)/lib64 \
	$(INITRAMFS)/bin \
	$(INITRAMFS)/sbin \
	$(INITRAMFS)/usr/bin \
	$(INITRAMFS_EMPTY_DIRS)
SYSCALL_TEST_DIR := $(INITRAMFS)/opt/syscall_test

.PHONY: all
all: build

$(INITRAMFS)/lib/x86_64-linux-gnu: | $(VDSO_LIB)
	@mkdir -p $@
	@# required for VDSO
	@cp -L $(VDSO_LIB) $@

$(VDSO_LIB): | $(VDSO_DIR) $(BINARY_CACHE_DIR)/vdso64.so
	@# TODO: use a custom compiled vdso.so file in the future.
	@cp $(BINARY_CACHE_DIR)/vdso64.so $@
	
$(VDSO_DIR):
	@mkdir -p $@
	
$(BINARY_CACHE_DIR)/vdso64.so:
	@mkdir -p $(BINARY_CACHE_DIR)
	@$(ATOMIC_WGET) $@ "https://raw.githubusercontent.com/asterinas/linux_vdso/2a6d2db/vdso64.so"

$(INITRAMFS)/lib64:
	@mkdir -p $@
	@cp -L /lib64/ld-linux-x86-64.so.2 $@

$(INITRAMFS)/etc:
	@mkdir -p $@
	@cp $(CUR_DIR)/etc/* $@

# Install busybox into /bin and /usr/bin.
$(INITRAMFS)/bin:
	@mkdir -p $@
	@wget -O busybox.tar.bz2 https://busybox.net/downloads/busybox-1.35.0.tar.bz2 \
	&& mkdir -p /root/download/busybox  \
	&& tar -xf busybox.tar.bz2 --strip-components=1 -C /root/download/busybox \
	&& rm busybox.tar.bz2
	@cd /root/download/busybox \
	&& CROSS_COMPILE=riscv64-unknown-linux-gnu- make  defconfig \
	&& sed -i "s/# CONFIG_STATIC is not set/CONFIG_STATIC=y/g" .config \
	&& sed -i 's|CONFIG_PREFIX="./_install"|CONFIG_PREFIX="/root/asterinas/test/build/initramfs"|g' .config \
	&& make CROSS_COMPILE=riscv64-unknown-linux-gnu-  install 
	

$(INITRAMFS)/sbin:
	@mkdir -p $@
	@cp /sbin/ldconfig $@
	@cp /sbin/ldconfig.real $@

$(INITRAMFS)/usr/bin: | $(INITRAMFS)/bin
	@mkdir -p $@
	@cp /usr/bin/busybox $@
# $(INITRAMFS)/usr/local:
# 	@mkdir -p $@
# 	@cp -r /usr/local/nginx $@
# 	@cp -r /usr/local/redis $@

# .PHONY: $(INITRAMFS)/test
# $(INITRAMFS)/test:
# 	@make --no-print-directory -C apps

$(INITRAMFS)/benchmark: | $(INITRAMFS)/benchmark/bin
	@cp -rf $(CUR_DIR)/benchmark/* $@

# $(INITRAMFS)/benchmark/bin:
# 	@mkdir -p $@
# 	@cp /usr/local/benchmark/sysbench/bin/sysbench $@
# 	@cp /usr/local/benchmark/iperf/bin/iperf3 $@
# 	@cp /usr/local/benchmark/membench/membench $@
# 	@cp /usr/local/benchmark/iozone $@
# 	@cp -r /usr/local/benchmark/unixbench $@
# 	@cp -r /usr/local/benchmark/lmbench $@
# 	@cp /usr/local/benchmark/fio/bin/fio $@
# 	@cp /usr/local/benchmark/sqlite-speedtest1 $@
# 	@cp /usr/local/leveldb/benchmark/db_bench $@
# 	@mv $@/db_bench $@/db_bench_leveldb
# 	@cp /usr/local/leveldb/benchmark/db_bench_sqlite3 $@

# Make necessary directories.
$(INITRAMFS_EMPTY_DIRS):
	@mkdir -p $@

.PHONY: $(SYSCALL_TEST_DIR)
$(SYSCALL_TEST_DIR):
	@make --no-print-directory -C syscall_test

.PHONY: $(INITRAMFS_IMAGE)
$(INITRAMFS_IMAGE): $(INITRAMFS_FILELIST)
	@if ! cmp -s $(INITRAMFS_FILELIST) $(INITRAMFS_FILELIST).previous ; then \
		echo "Generating the initramfs image..."; \
		cp -f $(INITRAMFS_FILELIST) $(INITRAMFS_FILELIST).previous; \
		( \
			cd $(INITRAMFS); \
			# `$(INITRAMFS_FILELIST)` contains files' last modification \
			# time in the first column, and files' relative path to \
			# `$(INITRAMFS)` in the second column. This prunes the first \
			# column and passes the second column to `cpio`. \
			cut -d " " -f 2- $(INITRAMFS_FILELIST) | \
				cpio -o -H newc | gzip \
		) > $@; \
	fi

.PHONY: $(INITRAMFS_FILELIST)
# If the BUILD_SYSCALL_TEST variable is set, we should depend on the
# sub make output to do incremental building.
ifeq ($(BUILD_SYSCALL_TEST), 1)
$(INITRAMFS_FILELIST): | $(INITRAMFS_ALL_DIRS) $(SYSCALL_TEST_DIR)
else
$(INITRAMFS_FILELIST): | $(INITRAMFS_ALL_DIRS)
endif
	@(cd $(INITRAMFS); find . -printf "%T@ %p\n") > $(INITRAMFS_FILELIST)

$(EXT2_IMAGE):
	@dd if=/dev/zero of=$(EXT2_IMAGE) bs=2G count=1
	@mke2fs $(EXT2_IMAGE)

$(EXFAT_IMAGE):
	@fallocate -l 64M $(EXFAT_IMAGE)
	@mkfs.exfat $(EXFAT_IMAGE)

.PHONY: build
build: $(INITRAMFS_IMAGE) $(EXT2_IMAGE) $(EXFAT_IMAGE)

.PHONY: format
format:
	@make --no-print-directory -C apps format

.PHONY: check
check:
	@make --no-print-directory -C apps check

.PHONY: clean
clean:
	@rm -rf $(BUILD_DIR)