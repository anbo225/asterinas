MKFILE_PATH := $(abspath $(lastword $(MAKEFILE_LIST)))
CUR_DIR := $(patsubst %/,%,$(dir $(MKFILE_PATH)))


SRC_DIR := /home/anbo/workspace/redis
INITRAMFS ?= $(CUR_DIR)/../../build/initramfs
TARGET_DIR := $(INITRAMFS)/redis

.PHONY: all 


# $(TESTS): $(BIN_DIR) $(TARGET_DIR)
# 	@cp -f $</$@ $(TARGET_DIR)/tests
# 	@make && make PREFIX=$@ install


all: $(SRC_DIR)  
	echo $(TARGET_DIR) 
	@cd $(SRC_DIR)  && make && make PREFIX=$(TARGET_DIR) install