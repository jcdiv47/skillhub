#!/bin/sh
# Builds SkillHub.app on a Mac: scripts/mac-app.sh [--install]
# The bundle lands in build/SkillHub.app; --install copies it to /Applications.
set -eu
cd "$(dirname "$0")/.."

cargo build --release
app=build/SkillHub.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/skillhub "$app/Contents/MacOS/skillhub"

# The icon, at every size macOS asks for.
set_dir=build/SkillHub.iconset
rm -rf "$set_dir" && mkdir -p "$set_dir"
for size in 16 32 128 256 512; do
    sips -z $size $size resources/icon.png --out "$set_dir/icon_${size}x${size}.png" >/dev/null
    sips -z $((size * 2)) $((size * 2)) resources/icon.png --out "$set_dir/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$set_dir" -o "$app/Contents/Resources/SkillHub.icns"
rm -rf "$set_dir"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cat >"$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>SkillHub</string>
    <key>CFBundleDisplayName</key><string>SkillHub</string>
    <key>CFBundleIdentifier</key><string>dev.skillhub.app</string>
    <key>CFBundleExecutable</key><string>skillhub</string>
    <key>CFBundleIconFile</key><string>SkillHub</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST
codesign --force --sign "${MAC_APP_SIGN:--}" "$app"
echo "$app"

if [ "${1:-}" = "--install" ]; then
    osascript -e 'tell application id "dev.skillhub.app" to quit' >/dev/null 2>&1 || true
    rm -rf /Applications/SkillHub.app
    cp -R "$app" /Applications/SkillHub.app
    echo /Applications/SkillHub.app
fi
