#!/bin/bash
# Build script for r2 AppImage
set -euo pipefail

echo "=== Building r2 AppImage ==="

# Build the project
echo "Building project..."
cargo build --release

# Create dist directory
mkdir -p dist

# Download linuxdeploy if not present
LINUXDEPLOY="./linuxdeploy-x86_64.AppImage"
if [ ! -f "$LINUXDEPLOY" ]; then
    echo "Downloading linuxdeploy..."
    wget -q https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage -O "$LINUXDEPLOY"
    chmod +x "$LINUXDEPLOY"
fi

# Download linuxdeploy-plugin-gtk if not present
GTK_PLUGIN="./linuxdeploy-plugin-gtk.sh"
if [ ! -f "$GTK_PLUGIN" ]; then
    echo "Downloading linuxdeploy-plugin-gtk..."
    wget -q https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh -O "$GTK_PLUGIN"
    chmod +x "$GTK_PLUGIN"
fi

# Create AppDir structure
echo "Creating AppDir..."
APPDIR="./AppDir"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$APPDIR/usr/share/metainfo"

# Copy binary and resources
cp target/release/r2 "$APPDIR/usr/bin/"
cp resources/r2.desktop "$APPDIR/usr/share/applications/"
cp resources/r2.metainfo.xml "$APPDIR/usr/share/metainfo/"

# Copy icon if exists
if [ -f resources/icons/hicolor/256x256/apps/r2.png ]; then
    cp resources/icons/hicolor/256x256/apps/r2.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/"
fi

# Bundle GTK4 and dependencies using linuxdeploy
echo "Bundling dependencies..."
export LDAI_OUTPUT="r2-x86_64.AppImage"
ARCH=x86_64 ./linuxdeploy-x86_64.AppImage --appdir "$APPDIR" --plugin gtk --output appimage

# Move AppImage to dist/
if [ -f "r2-x86_64.AppImage" ]; then
    mv r2-x86_64.AppImage ./dist/
    echo "✅ AppImage built: ./dist/r2-x86_64.AppImage"
else
    # Try to find the generated AppImage
    ls -la *.AppImage 2>/dev/null && mv *.AppImage ./dist/ 2>/dev/null || true
    echo "⚠️  AppImage may have been built with a different name. Check ./dist/"
fi

echo "=== Build complete ==="
echo "AppImage: dist/*.AppImage"
