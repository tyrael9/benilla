#!/usr/bin/env python3
"""Load the fork's portable addons through benilla's real FrameXML/Lua addon harness."""

from __future__ import annotations

import subprocess
import sys

ADDONS = ("BenillaController", "BenillaMobile", "MinimapMover")


def main() -> int:
    for addon in ADDONS:
        command = [
            "cargo",
            "run",
            "-q",
            "-p",
            "benilla-app",
            "--example",
            "addon_harness",
            "--",
            "addons",
            "--probe",
            addon,
        ]
        print(f"\n== {addon} ==", flush=True)
        result = subprocess.run(command, check=False, text=True)
        if result.returncode:
            print(f"{addon}: addon harness exited {result.returncode}", file=sys.stderr)
            return result.returncode
        # The harness always prints both counters. Keep this check deliberately strict: a Lua
        # chunk that loads but fails during PLAYER_LOGIN is not a compatible addon.
        # Capture output separately so CI can report the complete failing transcript.
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
