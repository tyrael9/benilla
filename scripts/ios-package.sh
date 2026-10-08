#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TARGET="aarch64-apple-ios"
PROFILE="${PROFILE:-release}"
APP="${IOS_APP_DIR:-$ROOT/target/$TARGET/$PROFILE/Benilla.app}"
BINARY="$ROOT/target/$TARGET/$PROFILE/benilla"
BUNDLE_ID="${IOS_BUNDLE_ID:-com.tyrael9.benilla}"
DEVICE_UDID="${IOS_DEVICE_UDID:-}"

echo "==> Building Benilla for iPhone..."
cargo build -p benilla --profile "$PROFILE" --target "$TARGET"

test -x "$BINARY" || { echo "error: missing $BINARY" >&2; exit 1; }

rm -rf "$APP"
mkdir -p "$APP"

cp "$BINARY" "$APP/benilla"
cp "$ROOT/ios/Info.plist" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $BUNDLE_ID" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $(git rev-list --count HEAD)" "$APP/Info.plist"

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
    echo "error: no Apple code-signing identity found." >&2
    exit 2
fi

PROFILE_PATH="${IOS_PROVISIONING_PROFILE:-}"
if [[ -n "$PROFILE_PATH" && ! -f "$PROFILE_PATH" ]]; then
    echo "error: provisioning profile not found: $PROFILE_PATH" >&2
    exit 3
fi

# Automatic signing profiles created by Xcode are often kept inside DerivedData
# rather than ~/Library/MobileDevice/Provisioning Profiles. Search both places
# and select a profile whose application-identifier matches this bundle ID.
if [[ -z "$PROFILE_PATH" ]]; then
    PROFILE_CANDIDATES="$(
        {
            find "$HOME/Library/MobileDevice/Provisioning Profiles" -type f -name '*.mobileprovision' -print 2>/dev/null || true
            find "$HOME/Library/Developer/Xcode/DerivedData" -type f -name 'embedded.mobileprovision' -print 2>/dev/null || true
        } | sort -u
    )"

    while IFS= read -r p; do
        [[ -z "$p" ]] && continue
        TEST_PLIST="$TMPDIR_PROFILE_CHECK/profile.plist"
        rm -rf "$TMPDIR_PROFILE_CHECK"
        mkdir -p "$TMPDIR_PROFILE_CHECK"
        if /usr/bin/security cms -D -i "$p" -o "$TEST_PLIST" 2>/dev/null; then
            TEST_APP_ID="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:application-identifier' "$TEST_PLIST" 2>/dev/null || true)"
            TEST_BUNDLE_ID="${TEST_APP_ID#*.}"
            if [[ "$TEST_BUNDLE_ID" == "$BUNDLE_ID" || "$TEST_BUNDLE_ID" == "*" ]]; then
                PROFILE_PATH="$p"
                break
            fi
        fi
    done <<< "$PROFILE_CANDIDATES"
fi

if [[ -z "$PROFILE_PATH" || ! -f "$PROFILE_PATH" ]]; then
    echo "error: no provisioning profile matching $BUNDLE_ID found." >&2
    echo "If Xcode generated one, it may be embedded in DerivedData; otherwise pass IOS_PROVISIONING_PROFILE=..." >&2
    exit 3
fi

TMPDIR_SIGN="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_SIGN"' EXIT
PROFILE_PLIST="$TMPDIR_SIGN/profile.plist"
ENTITLEMENTS="$TMPDIR_SIGN/entitlements.plist"

/usr/bin/security cms -D -i "$PROFILE_PATH" -o "$PROFILE_PLIST"

TEAM_ID="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.developer.team-identifier' "$PROFILE_PLIST" 2>/dev/null || true)"
PROFILE_APP_ID="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:application-identifier' "$PROFILE_PLIST" 2>/dev/null || true)"
PROFILE_BUNDLE_ID="${PROFILE_APP_ID#*.}"

echo "==> Signing identity: $IDENTITY"
echo "==> Provisioning profile: $PROFILE_PATH"
echo "    Team ID: $TEAM_ID"
echo "    App ID:  $PROFILE_APP_ID"

if [[ -z "$TEAM_ID" || -z "$PROFILE_APP_ID" ]]; then
    echo "error: provisioning profile has no usable app/team identifier." >&2
    exit 4
fi

if [[ "$PROFILE_BUNDLE_ID" != "*" && "$PROFILE_BUNDLE_ID" != "$BUNDLE_ID" ]]; then
    echo "error: provisioning profile does not match $BUNDLE_ID (profile is $PROFILE_APP_ID)." >&2
    echo "Use IOS_PROVISIONING_PROFILE=... with a matching profile, or set IOS_BUNDLE_ID." >&2
    exit 5
fi

/usr/bin/plutil -extract Entitlements xml1 -o "$ENTITLEMENTS" "$PROFILE_PLIST"
/usr/libexec/PlistBuddy -c "Set :application-identifier $TEAM_ID.$BUNDLE_ID" "$ENTITLEMENTS"

cp "$PROFILE_PATH" "$APP/embedded.mobileprovision"

echo "==> Signing..."
codesign --force --sign "$IDENTITY" --timestamp=none \
    --entitlements "$ENTITLEMENTS" "$APP/benilla"

echo "==> Verifying signature..."
codesign --verify --deep --strict --verbose=2 "$APP"

if [[ "${IOS_INSTALL:-0}" == "1" ]]; then
    if [[ -z "$DEVICE_UDID" ]]; then
        DEVICE_UDID="$(xcrun devicectl list devices 2>/dev/null |
            awk '/connected/ {print $3; exit}' || true)"
    fi
    if [[ -z "$DEVICE_UDID" ]]; then
        echo "error: no connected iPhone found." >&2
        exit 6
    fi
    echo "==> Installing on $DEVICE_UDID..."
    xcrun devicectl device install app --device "$DEVICE_UDID" "$APP"
fi

echo
echo "==> Done: $APP"
