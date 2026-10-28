#!/bin/sh
# Build "Canvas Lite.app" (release), with icon, signed.
#   ./dev/bundle.sh            build into target/
#   ./dev/bundle.sh --install  also copy to /Applications
# Signs with the first "Developer ID Application" identity in the keychain,
# or ad-hoc if there isn't one.
set -e
cd "$(dirname "$0")/.."
cargo build --release
APP="target/Canvas Lite.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/canvas-lite "$APP/Contents/MacOS/canvas-lite"
cp assets/icon/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Canvas Lite</string>
  <key>CFBundleDisplayName</key><string>Canvas Lite</string>
  <key>CFBundleIdentifier</key><string>dev.canvaslite.app</string>
  <key>CFBundleExecutable</key><string>canvas-lite</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.education</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
</dict></plist>
PLIST

IDENTITY=$(security find-identity -v -p codesigning | grep "Developer ID Application" | head -1 | sed -E 's/.*"(.*)"/\1/')
if [ -n "$IDENTITY" ]; then
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$APP"
  echo "Signed with: $IDENTITY"
else
  codesign --force --sign - "$APP"
  echo "Signed ad-hoc (no Developer ID found)"
fi
echo "Built $APP"

if [ "$1" = "--install" ]; then
  osascript -e 'quit app id "dev.canvaslite.app"' >/dev/null 2>&1 || true
  sleep 1
  rm -rf "/Applications/Canvas Lite.app"
  cp -R "$APP" /Applications/
  echo "Installed /Applications/Canvas Lite.app"
fi
