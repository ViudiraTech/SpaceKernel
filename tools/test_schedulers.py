#!/usr/bin/env python3
#
#       tools/test_schedulers.py
#       Uniform debug/release SMP scheduler validation for all kernel architectures
#
#       2026/9/30 By JiTianYu391
#       Copyright (C) 2026 ViudiraTech.
#

"""Run identical boot/algorithm/scheduler tests, recording every case and failure."""

import argparse
import json
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profiles", nargs="+", choices=["debug", "release"], default=["debug", "release"])
    parser.add_argument("--cpus", nargs="+", type=int, default=[1, 4])
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    output = root / "build" / "sched-tests"
    output.mkdir(parents=True, exist_ok=True)
    results = []
    cases = [(arch, profile, cpus, "default")
             for profile in args.profiles
             for arch in ["x86_64", "aarch64", "riscv64"] for cpus in args.cpus]
    cases += [(arch, profile, max(args.cpus), variant)
              for profile in args.profiles
              for arch, variant in [("aarch64", "gicv3"), ("riscv64", "sv48")]]
    for arch, profile, cpus, variant in cases:
        name = f"{arch}-{profile}-{cpus}cpu-{variant}"
        logfile = output / f"{name}.log"
        command = ["make", "test", f"ARCH={arch}", f"PROFILE={profile}", f"CPUS={cpus}",
                   "CONFIG_BOOT_SELF_TEST=y", "CONFIG_RISCV_SV48=y" if variant == "sv48" else "CONFIG_RISCV_SV48=n"]
        if variant == "gicv3":
            command.append("MACHINE=virt,gic-version=3")
        print(f"RUN {name}", flush=True)
        start = time.monotonic()
        with logfile.open("w") as log:
            result = subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT)
        elapsed = round(time.monotonic() - start, 2)
        passed = result.returncode == 0
        results.append({"case": name, "passed": passed, "seconds": elapsed,
                        "log": str(logfile), "command": command})
        print(f"{'PASS' if passed else 'FAIL'} {name} ({elapsed}s)", flush=True)
        if not passed:
            print("\n".join(logfile.read_text(errors="replace").splitlines()[-25:]), flush=True)
    report = output / "results.json"
    report.write_text(json.dumps(results, indent=2) + "\n")
    passed = sum(case["passed"] for case in results)
    print(f"{passed}/{len(results)} scheduler cases passed; report: {report}", flush=True)
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
