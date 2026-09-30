#!/usr/bin/env python3
#
#       tools/qemu_test.py
#       Boot the generated ISO and require the kernel's self-test marker
#
#       2026/9/30 By JiTianYu391
#       Copyright (C) 2026 ViudiraTech.
#

"""Boot the generated ISO and require the kernel's self-test marker."""

import selectors
import argparse
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=["x86_64", "aarch64", "riscv64"])
    for argument in ["iso", "firmware", "cpus", "memory_mib"]:
        parser.add_argument(argument)
    parser.add_argument("--accel", choices=["tcg", "kvm"])
    parser.add_argument("--cpu")
    parser.add_argument("--machine")
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--online-cpus", type=int)
    parser.add_argument("--require", action="append", default=[])
    args = parser.parse_args()
    arch, iso, firmware, cpus, memory_mib = args.arch, args.iso, args.firmware, args.cpus, args.memory_mib
    binary = {
        "x86_64": "qemu-system-x86_64",
        "aarch64": "qemu-system-aarch64",
        "riscv64": "qemu-system-riscv64",
    }[arch]
    machine = args.machine or ("q35" if arch == "x86_64" else "virt")
    command = [binary, "-machine", machine, "-m", memory_mib, "-smp", cpus,
               "-display", "none", "-serial", "stdio", "-monitor", "none", "-no-reboot",
               "-drive", f"if=pflash,format=raw,readonly=on,file={firmware}",
               "-cdrom", iso]
    cpu = args.cpu or ("max" if arch == "aarch64" else None)
    if cpu:
        command += ["-cpu", cpu]
    if args.accel:
        command += ["-accel", args.accel]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    output = bytearray()
    deadline = time.monotonic() + args.timeout
    if args.online_cpus is not None:
        args.require.append(f"SMP: {args.online_cpus}/{args.online_cpus} CPUs online")
    try:
        while time.monotonic() < deadline:
            for key, _ in selector.select(timeout=0.5):
                chunk = key.fileobj.read1(4096)
                if chunk:
                    output.extend(chunk)
                    if b"KERNEL EMERGENCY" in output:
                        deadline = min(deadline, time.monotonic() + 2)
                    if b"BOOT_OK" in output and all(marker.encode() in output for marker in args.require):
                        print(output.decode(errors="replace"))
                        return 0
            if process.poll() is not None:
                break
        print(output.decode(errors="replace"), file=sys.stderr)
        print("QEMU boot test failed: required markers not seen", file=sys.stderr)
        return 1
    finally:
        selector.close()
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


if __name__ == "__main__":
    sys.exit(main())
