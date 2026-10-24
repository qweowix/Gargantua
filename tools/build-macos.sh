#!/bin/bash
# Сборка BlackHole.app на маке: универсальный бинарь (Apple Silicon + Intel) одним файлом.
# Нужен Rust через rustup: curl https://sh.rustup.rs -sSf | sh
# Результат: dist/BlackHole.app и dist/BlackHole-macos.zip
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add aarch64-apple-darwin x86_64-apple-darwin
export MACOSX_DEPLOYMENT_TARGET=11.0
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

APP=dist/BlackHole.app
rm -rf dist && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/BlackHole" \
  target/aarch64-apple-darwin/release/blackhole target/x86_64-apple-darwin/release/blackhole

# иконка: из png делаем .icns штатными sips + iconutil
ICON=dist/icon.iconset
mkdir -p "$ICON"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/icon-512.png --out "$ICON/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2)); [ $d -le 512 ] && sips -z $d $d assets/icon-512.png --out "$ICON/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICON" -o "$APP/Contents/Resources/BlackHole.icns"
rm -rf "$ICON"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>BlackHole</string>
  <key>CFBundleDisplayName</key><string>BlackHole</string>
  <key>CFBundleIdentifier</key><string>dev.qweowix.blackhole</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleExecutable</key><string>BlackHole</string>
  <key>CFBundleIconFile</key><string>BlackHole</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

# ad-hoc подпись: без неё на Apple Silicon бинарь вообще не запустится
codesign --force --deep -s - "$APP"
(cd dist && ditto -c -k --keepParent BlackHole.app BlackHole-macos.zip)
echo "готово: dist/BlackHole-macos.zip"
