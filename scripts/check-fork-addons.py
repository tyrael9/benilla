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
        if addon == "BenillaController":
            slot_regression = r"""
                local button = getglobal("BenillaControllerButton3")
                local invoked = nil
                UseAction = function(slot) invoked = slot end
                HasAction = function() return false end
                GetBonusBarOffset = function() return 0 end
                UpdateAllSlots()
                if button.actionSlot ~= 3 then error("normal page display mismatch") end
                BenillaController_Slot(3)
                if invoked ~= 3 then error("normal page activation mismatch") end
                GetBonusBarOffset = function() return 2 end
                UpdateAllSlots()
                if button.actionSlot ~= 87 then error("stance page display mismatch") end
                BenillaController_Slot(3)
                if invoked ~= 87 then error("stance page activation mismatch") end
                BenillaController_SetModifier(1)
                UpdateAllSlots()
                if button.actionSlot ~= 11 then error("L-layer display mismatch") end
                BenillaController_Slot(11)
                if invoked ~= 11 then error("L-layer activation mismatch") end
                BenillaController_SetModifier(2)
                UpdateAllSlots()
                if button.actionSlot ~= 19 then error("R-layer display mismatch") end
                BenillaController_Slot(19)
                if invoked ~= 19 then error("R-layer activation mismatch") end
                return "PASS"
            """
            command.extend(["--eval", slot_regression])
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
        if addon == "BenillaController" and "= PASS" not in transcript:
            print("BenillaController: stance/action-slot regression probe did not pass", file=sys.stderr)
            return 1
    print("\nAll fork addons loaded and survived session startup.", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
