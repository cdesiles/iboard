#!/bin/sh
# Assembles iboard.app: the release binary, a full Info.plist and an .icns
# made from the 1024 Logo master, then ad-hoc signs it so Gatekeeper is quiet.
#
#   macos/bundle.sh <binary> <icon.png> <version> <iboard.app>

set -eu

binary=$1
icon=$2
version=$3
app=$4

rm -rf "$app"
contents="$app/Contents"
mkdir -p "$contents/MacOS" "$contents/Resources"
cp "$binary" "$contents/MacOS/iboard"

iconset="target/iboard.iconset"
rm -rf "$iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    double=$((size * 2))
    sips -z "$size" "$size" "$icon" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$double" "$double" "$icon" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$contents/Resources/iboard.icns"

cat > "$contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>iboard</string>
	<key>CFBundleDisplayName</key>
	<string>iboard</string>
	<key>CFBundleIdentifier</key>
	<string>com.cdesiles.iboard</string>
	<key>CFBundleExecutable</key>
	<string>iboard</string>
	<key>CFBundleIconFile</key>
	<string>iboard</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$version</string>
	<key>CFBundleVersion</key>
	<string>$version</string>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>LSMinimumSystemVersion</key>
	<string>12.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
EOF

codesign --force --sign - "$app"
