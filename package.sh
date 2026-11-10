#!/bin/sh
# Builds the finished program into export/:
#   export/Mac/Gravador de Imagens.app   (and a .zip of it)
#   export/Windows/Gravador de Imagens.exe (when the Windows toolchain is installed)
set -e
cd "$(dirname "$0")"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
mkdir -p export

# ---- macOS
cargo build --release
APP="export/Mac/Gravador de Imagens.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/gravador-guiado "$APP/Contents/MacOS/gravador-guiado"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Gravador de Imagens</string>
  <key>CFBundleDisplayName</key><string>Gravador de Imagens</string>
  <key>CFBundleIdentifier</key><string>local.gravador-guiado</string>
  <key>CFBundleExecutable</key><string>gravador-guiado</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDevelopmentRegion</key><string>pt-BR</string>
</dict>
</plist>
PLIST
codesign --force --deep -s - "$APP" 2>/dev/null || true
cp COMO-USAR.txt NOTICE.md export/Mac/ 2>/dev/null || true
( cd export/Mac && ditto -c -k --keepParent "Gravador de Imagens.app" "Gravador de Imagens - Mac.zip" )
echo "Mac: $APP"

# ---- Windows (needs: rustup target add x86_64-pc-windows-gnu ; brew install mingw-w64)
if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
  cargo build --release --target x86_64-pc-windows-gnu
  mkdir -p export/Windows
  cp target/x86_64-pc-windows-gnu/release/gravador-guiado.exe "export/Windows/Gravador de Imagens.exe"
  cp COMO-USAR.txt NOTICE.md export/Windows/ 2>/dev/null || true
  ( cd export/Windows && zip -q -r "Gravador de Imagens - Windows.zip" "Gravador de Imagens.exe" COMO-USAR.txt NOTICE.md )
  echo "Windows: export/Windows/Gravador de Imagens.exe"
else
  echo "Windows: toolchain not installed, skipped"
fi
