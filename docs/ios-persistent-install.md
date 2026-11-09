# iOS: persistent WoW install and addons

The iOS app bundle is read-only and is replaced when the app is updated. Player data must therefore
live in the app's writable container, not under `Benilla.app`.

## Persistent locations

The app resolves these paths from its iOS sandbox `$HOME`:

- WoW archives: `Library/Application Support/Benilla/Data/`
- Player state and addons: `Library/Application Support/Benilla/benilla-config/`
- Loose addons: `Library/Application Support/Benilla/benilla-config/AddOns/`

The addon state, saved variables, macros, bindings and config all stay under
`benilla-config/`. Refreshing the app-bundled addons merges their files into `AddOns/`: packaged
files are updated, but addon folders that are not part of the bundle are kept.

## First install (or when the Data directory is absent)

Include Data in the app bundle once. The app copies it to its persistent location on first launch,
opens the staged MPQ chain to validate it, and only then activates it. A ready marker prevents
subsequent launches from copying the multi-gigabyte folder again.

From the repository root on the Mac:

```sh
IOS_BUNDLE_DATA=1 \
WOW_DATA=/path/to/WoW/Data \
IOS_BUNDLE_ADDONS=/path/to/benilla-config/AddOns \
IOS_INSTALL=1 \
bash scripts/ios-package.sh
```

A valid existing persistent Data directory without the marker is adopted after validation. An
existing invalid directory is left untouched and reported; the app does not silently delete or
replace it.

## Subsequent app updates

Do **not** set `IOS_BUNDLE_DATA=1` again. The package script recreates the local `.app` output on
each build, so omitting this flag keeps the large Data tree out of future app updates. The running
app finds the persistent copy automatically.

When you want to update bundled addons too, keep `IOS_BUNDLE_ADDONS` set:

```sh
IOS_BUNDLE_ADDONS=/path/to/benilla-config/AddOns \
IOS_INSTALL=1 \
bash scripts/ios-package.sh
```

When the addon bundle is omitted, the existing persistent addon directory is left alone. The app
update itself preserves the sandbox. Do not delete the Benilla app from the device as a way to
install an update: deleting the app also deletes its container data.

## Data path overrides

On iOS, `WOW_DATA` still takes precedence over the persistent default. `BENILLA_HOME` still
overrides the `benilla-config` directory, including the AddOns path.
