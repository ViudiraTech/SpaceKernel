#!/usr/bin/env python3
#
#       tools/test_matrix.py
#       Run host model checks plus the three-architecture boot matrix and
#       print a summary derived from real exit codes and QEMU result files.
#
#       2026/10/25 By JiTianYu391
#       Copyright (C) 2026 ViudiraTech.
#

"""Run the host model checks and the three-architecture QEMU boot matrix."""

import json
import os
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
RESULTS = ROOT / "build" / "test-results"
ARCHES = ("x86_64", "aarch64", "riscv64")
HOST_CHECKS = (
    ("sched-model", "test-sched-model"),
    ("subsystem-model", "test-subsystem-model"),
    ("drm-abi", "test-drm-abi"),
)


def tail(text, lines=40):
    return "\n".join(text.splitlines()[-lines:])


def run(command, env, log_name):
    RESULTS.mkdir(parents=True, exist_ok=True)
    log_path = RESULTS / log_name
    started = time.monotonic()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    log_path.write_text(completed.stdout, encoding="utf-8")
    return {
        "passed": completed.returncode == 0,
        "returncode": completed.returncode,
        "log": str(log_path.relative_to(ROOT)),
        "output": completed.stdout,
        "elapsed": time.monotonic() - started,
    }


def report(label, item, detail=""):
    status = "PASS" if item["passed"] else "FAIL"
    print(f"[{status}] {label} ({item['elapsed']:.1f}s) {detail}".rstrip())
    if not item["passed"]:
        print(tail(item["output"]))
        print(f"  log: {item['log']}")
    return item


def parse_selftest(summary):
    fields = {}
    for token in summary.split():
        if "=" in token:
            key, value = token.split("=", 1)
            fields[key] = value
    return fields


def read_result(path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def main():
    env = os.environ.copy()
    host = []
    for label, target in HOST_CHECKS:
        item = run(["make", target], env, f"host-{label}.log")
        report(f"host:{label}", item)
        host.append({"label": f"host:{label}", **item})

    boots = []
    for arch in ARCHES:
        result_path = RESULTS / f"{arch}.json"
        try:
            result_path.unlink()
        except FileNotFoundError:
            pass
        boot_env = env.copy()
        boot_env["QEMU_RESULT"] = str(result_path)
        item = run(
            ["make", "test-single", f"ARCH={arch}", "CONFIG_BOOT_SELF_TEST=y"],
            boot_env,
            f"boot-{arch}.log",
        )
        qemu = read_result(result_path)
        if not item["passed"]:
            item["reason"] = f"make exited {item['returncode']}"
        elif qemu is None:
            item["passed"] = False
            item["reason"] = "make succeeded but qemu_test.py wrote no result file"
        elif qemu.get("status") != "pass":
            item["passed"] = False
            item["reason"] = "QEMU result status is not pass"
        item["qemu"] = qemu
        detail = (qemu or {}).get("summary", "") or item.get("reason", "")
        report(f"boot:{arch}", item, detail)
        boots.append({"label": f"boot:{arch}", **item})

    print()
    print("=" * 78)
    print("SpaceOS-9 verification summary")
    print("  (rows are derived from subprocess exit codes and qemu_test.py JSON)")
    print("=" * 78)
    for item in host:
        print(f"  {item['label']:<28} {'PASS' if item['passed'] else 'FAIL'}")
    for item in boots:
        qemu = item["qemu"] or {}
        status = "PASS" if item["passed"] else "FAIL"
        detail = qemu.get("summary", "") or item.get("reason", "no SELFTEST_OK")
        print(f"  {item['label']:<28} {status:<5} {detail}")

    unverified = []
    for item in boots:
        arch = item["label"].split(":", 1)[1]
        qemu = item["qemu"] or {}
        fields = parse_selftest(qemu.get("summary", ""))
        rng = fields.get("rng", "unknown")
        gpu = fields.get("gpu", qemu.get("gpu", "unknown"))
        if rng != "hw":
            unverified.append(f"{arch}: entropy/RNG ({rng}); getrandom EAGAIN and ASLR not accepted")
        if gpu == "none":
            unverified.append(f"{arch}: GPU (no virtio-gpu device in this boot matrix)")
        elif "venus-abi" in gpu:
            unverified.append(
                f"{arch}: Venus rendering and host ring>0 fencing "
                "(kernel ABI smoke test only; no Mesa/Venus userspace)"
            )
    unverified.extend(
        [
            "real hardware GPU: unverified (QEMU virglrenderer only)",
            "userspace Mesa/Vulkan, libc and ld.so: not implemented",
        ]
    )
    print()
    print("Unverified / 未验收:")
    for item in unverified:
        print(f"  - {item}")

    failed = [item for item in host if not item["passed"]]
    failed += [item for item in boots if not item["passed"]]
    print()
    if failed:
        print(f"RESULT: FAIL ({len(failed)} sub-check(s) failed)")
        return 1
    print("RESULT: PASS (all executed sub-checks passed; see unverified list above)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
