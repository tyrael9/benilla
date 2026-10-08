#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TARGET="aarch64-apple-ios"
PROFILE="${PROFILE:-release}"
APP="${IOS_APP_DIR:-$ROOT/target/$TARGET/$PROFILE/Benilla.app}"
BINARY="$ROOT/target/$TARGET/$PROFILE/benilla"
BUNDLE_ID="${IOS_BUNDLE_ID:-com.tyrael9.benilla}"

echo "==> Building Benilla for iPhone..."
cargo build -p benilla --profile "$PROFILE" --target "$TARGET"

test -x "$BINARY" || { echo "error: missing $BINARY" >&2; exit 1; }

rm -rf "$APP"
mkdir -p "$APP"

cp "$BINARY" "$APP/benilla"
cp "$ROOT/ios/Info.plist" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $BUNDLE_ID" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $(git rev-list --count HEAD)" "$APP/Info.plist"

echo "==> App bundle:"
echo "    $APP"
echo "==> Executable:"
file "$APP/benilla"

if [[ "${IOS_UNSIGNED:-0}" == "1" ]]; then
    echo "==> Leaving app unsigned (IOS_UNSIGNED=1)"
    exit 0
fi

IDENTITY="${IOS_CODE_SIGN_IDENTITY:-}"
if [[ -z "$IDENTITY" ]]; then
    IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null |
        sed -n 's/^[[:space:]]*[0-9]*) \([0-9A-F]\{40\}\) .*$/\1/p' | head -1 || true)"
fi

if [[ -z "$IDENTITY" ]]; then
    echo
    echo "No Apple code-signing identity was found."
    echo "The .app is complete but unsigned."
    echo
    echo "To inspect identities:"
    echo "  security find-identity -v -p codesigning"
    echo
    echo "To build without signing:"
    echo "  IOS_UNSIGNED=1 scripts/ios-package.sh"
    exit 2
fi

echo "==> Signing with $IDENTITY..."
codesign --force --sign "$IDENTITY" --timestamp=none --deep "$APP"

echo "==> Signature:"
codesign -dvv "$APP" 2>&1 | sed -n '1,14p'

if command -v xcrun >/dev/null 2>&1 && xcrun devicectl list devices >/dev/null 2>&1; then
    echo
    echo "==> Connected devices:"
    xcrun devicectl list devices
    echo
    echo "Install with:"
    echo "  xcrun devicectl device install app --device <DEVICE_UDID> '$APP'"
fi
