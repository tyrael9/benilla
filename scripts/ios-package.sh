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
DATA_SOURCE="${WOW_DATA:-$ROOT/WoW/Data}"
ADDONS_SOURCE="${IOS_BUNDLE_ADDONS:-}"
INSTALL_ONLY=0
if [[ "${1:-}" == "--install-only" ]]; then
    INSTALL_ONLY=1
    shift
elif [[ "${1:-}" == "--bundle-addons" ]]; then
    [[ "$#" -eq 2 ]] || { echo "usage: $0 --bundle-addons /path/to/AddOns" >&2; exit 64; }
    ADDONS_SOURCE="$2"
    shift 2
fi
if [[ "$#" -ne 0 ]]; then
    echo "usage: $0 [--install-only] | [--bundle-addons /path/to/AddOns]" >&2
    exit 64
fi

if [[ "$INSTALL_ONLY" == "1" ]]; then
    if [[ ! -d "$APP" || ! -x "$APP/benilla" ]]; then
        echo "error: packaged app not found: $APP" >&2
        echo "Run $0 first, or omit --install-only." >&2
        exit 1
    fi
    if [[ -z "$DEVICE_UDID" ]]; then
        DEVICE_UDID="$(xcrun devicectl list devices 2>/dev/null | awk '/connected/ {print $3; exit}' || true)"
    fi
    if [[ -z "$DEVICE_UDID" ]]; then
        echo "error: no connected iPhone found." >&2
        exit 6
    fi
    echo "==> Installing existing app on $DEVICE_UDID..."
    xcrun devicectl device install app --device "$DEVICE_UDID" "$APP"
    echo
    echo "==> Done: $APP"
    exit 0
fi

echo "==> Building Benilla for iPhone..."
cargo build -p benilla --no-default-features --profile "$PROFILE" --target "$TARGET"

test -x "$BINARY" || { echo "error: missing $BINARY" >&2; exit 1; }

rm -rf "$APP"
mkdir -p "$APP"

cp "$BINARY" "$APP/benilla"
cp "$ROOT/ios/Info.plist" "$APP/Info.plist"

if [[ "${IOS_BUNDLE_DATA:-0}" == "1" ]]; then
    if [[ ! -d "$DATA_SOURCE" ]]; then
        echo "error: WoW Data directory not found: $DATA_SOURCE" >&2
        echo "Set WOW_DATA=/path/to/your/1.12.1/Data or provide $ROOT/WoW/Data." >&2
        exit 1
    fi
    echo "==> Bundling WoW Data for first-install bootstrap from: $DATA_SOURCE"
    mkdir -p "$APP/Data"
    rsync -a "$DATA_SOURCE/" "$APP/Data/"
else
    echo "==> Skipping WoW Data bundle (persistent iOS app container is used)"
fi

if [[ -n "$ADDONS_SOURCE" ]]; then
    if [[ ! -d "$ADDONS_SOURCE" ]]; then
        echo "error: AddOns directory not found: $ADDONS_SOURCE" >&2
        exit 1
    fi
    echo "==> Bundling AddOns for iOS bootstrap from: $ADDONS_SOURCE"
    mkdir -p "$APP/Interface"
    rm -rf "$APP/Interface/AddOns"
    mkdir -p "$APP/Interface/AddOns"
    rsync -a "$ADDONS_SOURCE/" "$APP/Interface/AddOns/"
else
    echo "==> Skipping AddOns bundle (persistent iOS app container is used)"
fi
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

# Validate profiles before using them. Xcode may keep managed profiles in
# UserData, while old embedded.mobileprovision files can remain in DerivedData.
# When several profiles match, prefer the valid one with the latest expiration.
TMPDIR_SIGN="$(mktemp -d)"
trap 'rm -rf "${TMPDIR_SIGN:-}"' EXIT
PROFILE_PLIST="$TMPDIR_SIGN/profile.plist"
ENTITLEMENTS="$TMPDIR_SIGN/entitlements.plist"
PROFILE_CHECK_PLIST="$TMPDIR_SIGN/profile-check.plist"
NOW_EPOCH="$(date +%s)"

if ! command -v python3 >/dev/null 2>&1; then
    echo "error: python3 is required to validate provisioning-profile expiration dates." >&2
    exit 3
fi

# Sets PROFILE_TEAM_ID, PROFILE_APP_ID and PROFILE_EXPIRATION_EPOCH on success.
profile_is_valid() {
    local candidate="$1"

    PROFILE_TEAM_ID=""
    PROFILE_APP_ID=""
    PROFILE_BUNDLE_SPEC=""
    PROFILE_EXPIRATION_EPOCH=""

    if ! /usr/bin/security cms -D -i "$candidate" -o "$PROFILE_CHECK_PLIST" 2>/dev/null; then
        return 1
    fi

    PROFILE_TEAM_ID="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.developer.team-identifier' "$PROFILE_CHECK_PLIST" 2>/dev/null || true)"
    PROFILE_APP_ID="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:application-identifier' "$PROFILE_CHECK_PLIST" 2>/dev/null || true)"
    if [[ -z "$PROFILE_TEAM_ID" || -z "$PROFILE_APP_ID" || "$PROFILE_APP_ID" != *.* ]]; then
        return 1
    fi

    # Support exact App IDs as well as Apple's wildcard App IDs.
    PROFILE_BUNDLE_SPEC="${PROFILE_APP_ID#*.}"
    if [[ "$BUNDLE_ID" != $PROFILE_BUNDLE_SPEC ]]; then
        return 1
    fi

    PROFILE_EXPIRATION_EPOCH="$(
        python3 - "$PROFILE_CHECK_PLIST" <<'PY'
import datetime
import plistlib
import sys

with open(sys.argv[1], "rb") as profile_file:
    expiration = plistlib.load(profile_file)["ExpirationDate"]

# Apple plist dates are UTC; plistlib represents them as naive UTC datetimes.
if expiration.tzinfo is None:
    expiration = expiration.replace(tzinfo=datetime.timezone.utc)
print(int(expiration.timestamp()))
PY
    )" || return 1

    [[ "$PROFILE_EXPIRATION_EPOCH" =~ ^[0-9]+$ ]] || return 1
    (( PROFILE_EXPIRATION_EPOCH > NOW_EPOCH ))
}

if [[ -n "$PROFILE_PATH" ]]; then
    # An explicitly selected profile is authoritative, but never let an
    # expired or wrong-app profile silently through.
    if ! profile_is_valid "$PROFILE_PATH"; then
        echo "error: provisioning profile is expired, invalid, or does not match $BUNDLE_ID: $PROFILE_PATH" >&2
        exit 3
    fi
else
    BEST_PROFILE_PATH=""
    BEST_EXPIRATION_EPOCH=0

    while IFS= read -r -d '' p; do
        if profile_is_valid "$p"; then
            if (( PROFILE_EXPIRATION_EPOCH > BEST_EXPIRATION_EPOCH )); then
                BEST_PROFILE_PATH="$p"
                BEST_EXPIRATION_EPOCH="$PROFILE_EXPIRATION_EPOCH"
            fi
        fi
    done < <(
        for profile_dir in \
            "$HOME/Library/MobileDevice/Provisioning Profiles" \
            "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"; do
            if [[ -d "$profile_dir" ]]; then
                find "$profile_dir" -type f -name '*.mobileprovision' -print0 2>/dev/null || true
            fi
        done
        DERIVED_DATA="$HOME/Library/Developer/Xcode/DerivedData"
        if [[ -d "$DERIVED_DATA" ]]; then
            find "$DERIVED_DATA" -type f -name 'embedded.mobileprovision' -print0 2>/dev/null || true
        fi
    )

    if [[ -z "$BEST_PROFILE_PATH" ]]; then
        echo "error: no unexpired provisioning profile matching $BUNDLE_ID was found." >&2
        echo "Searched MobileDevice, Xcode UserData, and Xcode DerivedData; set IOS_PROVISIONING_PROFILE=... to select one explicitly." >&2
        exit 3
    fi

    PROFILE_PATH="$BEST_PROFILE_PATH"
fi

# Decode the selected profile again for signing and final verification.
/usr/bin/security cms -D -i "$PROFILE_PATH" -o "$PROFILE_PLIST"
TEAM_ID="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.developer.team-identifier' "$PROFILE_PLIST" 2>/dev/null || true)"
PROFILE_APP_ID="$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:application-identifier' "$PROFILE_PLIST" 2>/dev/null || true)"
PROFILE_BUNDLE_SPEC="${PROFILE_APP_ID#*.}"

echo "==> Signing identity: $IDENTITY"
echo "==> Provisioning profile: $PROFILE_PATH"
echo "    Team ID: $TEAM_ID"
echo "    App ID:  $PROFILE_APP_ID"
echo "    Expires: $(/usr/libexec/PlistBuddy -c 'Print :ExpirationDate' "$PROFILE_PLIST" 2>/dev/null || echo unknown)"

if [[ -z "$TEAM_ID" || -z "$PROFILE_APP_ID" ]]; then
    echo "error: provisioning profile has no usable app/team identifier." >&2
    exit 4
fi

if [[ "$BUNDLE_ID" != $PROFILE_BUNDLE_SPEC ]]; then
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
