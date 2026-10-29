#!/bin/sh
# Rust-ядро под телефон (arm64) → android/app/src/main/jniLibs. Нужны rustup с целью aarch64-linux-android
# и Android NDK (ANDROID_NDK_HOME или ~/Android/Sdk/ndk/<версия>). Ключи Firebase — из окружения или .cargo/config.toml
set -e
cd "$(dirname "$0")/core"
NDK=${ANDROID_NDK_HOME:-$(ls -d "$HOME"/Android/Sdk/ndk/* | sort -V | tail -1)}
BIN="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
API=26
export CC_aarch64_linux_android="$BIN/aarch64-linux-android$API-clang"
export AR_aarch64_linux_android="$BIN/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
PATH="$HOME/.cargo/bin:$PATH" nice -n 19 cargo build --release --target aarch64-linux-android
OUT=../android/app/src/main/jniLibs/arm64-v8a
mkdir -p "$OUT"
cp target/aarch64-linux-android/release/libblackhole_core.so "$OUT/"
ls -la "$OUT"
