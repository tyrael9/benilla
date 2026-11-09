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

## First install and Data updates

Include Data in the app bundle when installing for the first time. The app copies it to its
persistent location, opens the staged MPQ chain to validate it, and only then activates it.

The same flag can also be used in a later app update when the bundle contains newer or previously
missing files (for example, `eula.html` or `tos.html`). On launch, the app merges new and changed
packaged files into the persistent Data directory. Files not present in the bundle are retained.
Unchanged files, including the large MPQ archives, are skipped by comparing their size and
modification time; copied files retain the bundle's modification time so later launches can skip
them too. The ready marker no longer prevents Data refreshes.

From the repository root on the Mac:

```sh
IOS_BUNDLE_DATA=1 \
WOW_DATA=/path/to/WoW/Data \
IOS_BUNDLE_ADDONS=/path/to/benilla-config/AddOns \
IOS_INSTALL=1 \
bash scripts/ios-package.sh
```

A valid existing persistent Data directory without the marker is adopted after validation and
then merged with the bundle. An existing invalid unmarked directory is left untouched and
reported; the app does not silently delete or replace it.

## Subsequent app updates

When an update does not need to refresh Data, omit `IOS_BUNDLE_DATA=1`; the package script
recreates the local `.app` output on each build, and the app uses its persistent Data copy.
Include the flag again only when you want files from the current `WOW_DATA` source merged into
the installed Data directory.

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
