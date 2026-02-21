MACH := ./mach-havi

# Output directories
DIST := dist
DIST_LINUX := $(DIST)/linux-x86_64
DIST_ANDROID := $(DIST)/android-aarch64

# Build profile for dist targets: dev (default) or release
DIST_PROFILE ?= dev
ifeq ($(DIST_PROFILE),release)
MACH_PROFILE_FLAG := --release
MACH_PROFILE_DIR := release
else
MACH_PROFILE_FLAG :=
MACH_PROFILE_DIR := debug
endif

.PHONY: all build clean dist dist-linux dist-android test

all: build

build:
	$(MACH) build

test: build
	$(MAKE) -j$(shell python3 -c 'import os;print(min(os.cpu_count(),6))') -C tests/havi

# Linux dist build
dist-linux: $(DIST_LINUX)/havi

$(DIST_LINUX)/havi: | $(DIST_LINUX)
	$(MACH) build $(MACH_PROFILE_FLAG)
	cp target/$(MACH_PROFILE_DIR)/havi $@

$(DIST_LINUX):
	mkdir -p $@

# Android dist build
dist-android: $(DIST_ANDROID)/servo.apk

$(DIST_ANDROID)/servo.apk: | $(DIST_ANDROID)
	$(MACH) build $(MACH_PROFILE_FLAG) --android
	cp target/android/aarch64-linux-android/$(MACH_PROFILE_DIR)/servoapp.apk $@

$(DIST_ANDROID):
	mkdir -p $@

# Tarballs
tarball-linux: dist-linux
	tar -C $(DIST) -czf $(DIST)/havi-linux-x86_64.tar.gz linux-x86_64

tarball-android: dist-android
	tar -C $(DIST) -czf $(DIST)/havi-android-aarch64.tar.gz android-aarch64

clean:
	rm -rf $(DIST)

# Android emulator targets
ANDROID_SDK ?= $(HOME)/Android-Sdk
EMULATOR := $(ANDROID_SDK)/emulator/emulator
AVD_NAME := havi-test
APK_PATH := target/android/aarch64-linux-android/debug/servoapp.apk
SYSTEM_IMAGE := system-images;android-36;google_apis_playstore;x86_64

android-avd:
	@if ! $(ANDROID_SDK)/cmdline-tools/latest/bin/avdmanager list avd 2>/dev/null | grep -q "$(AVD_NAME)"; then \
		echo "Creating AVD $(AVD_NAME)..."; \
		$(ANDROID_SDK)/cmdline-tools/latest/bin/sdkmanager "$(SYSTEM_IMAGE)" && \
		echo "no" | $(ANDROID_SDK)/cmdline-tools/latest/bin/avdmanager create avd \
			-n $(AVD_NAME) \
			-k "$(SYSTEM_IMAGE)" \
			-d pixel_6; \
	else \
		echo "AVD $(AVD_NAME) already exists"; \
	fi

android-build:
	$(MACH) build --dev --target x86_64-linux-android

android-emulator: android-avd android-build
	@echo "Starting emulator..." && \
	$(EMULATOR) -avd $(AVD_NAME) -no-snapshot-load & EMU_PID=$$!; \
	trap 'kill $$EMU_PID 2>/dev/null || true' EXIT INT TERM; \
	echo "Waiting for device to boot..." && \
	adb wait-for-device && \
	while [ "$$(adb shell getprop sys.boot_completed 2>/dev/null)" != "1" ]; do sleep 2; done && \
	echo "Installing $(APK_PATH)..." && \
	adb install -r $(APK_PATH) && \
	trap - EXIT INT TERM && \
	echo "Done. HAVI installed on emulator (PID: $$EMU_PID)."

android-emulator-stop:
	@adb emu kill 2>/dev/null || true
