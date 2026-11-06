#!/usr/bin/env python3
"""Load the fork's portable addons through benilla's real FrameXML/Lua addon harness."""

from __future__ import annotations

import subprocess
import sys

ADDONS = ("BenillaController", "BenillaMobile", "MinimapMover")


def main() -> int:
    for addon in ADDONS:
        command = [
            "cargo", "run", "-q", "-p", "benilla-app",
            "--example", "addon_harness", "--",
            "addons", "--probe", addon,
        ]
        print(f"\n== {addon} ==", flush=True)
        result = subprocess.run(command, check=False, text=True, capture_output=True)
        transcript = result.stdout + result.stderr
        print(transcript, end="" if transcript.endswith("\n") else "\n")
        if result.returncode:
            print(f"{addon}: addon harness exited {result.returncode}", file=sys.stderr)
            return result.returncode
        for counter in ("load errors: 0", "session errors: 0"):
            if counter not in transcript:
                print(f"{addon}: expected {counter!r}; addon is not yet verified", file=sys.stderr)
                return 1
    print("\nAll fork addons loaded and survived session startup.", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
