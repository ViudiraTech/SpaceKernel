#!/usr/bin/env python3
"""Boot the generated ISO and require the kernel's self-test marker."""

import selectors
import subprocess
import sys
import time


def main():
    arch, iso, firmware, cpus, memory_mib = sys.argv[1:]
    binary = {
        "x86_64": "qemu-system-x86_64",
        "aarch64": "qemu-system-aarch64",
        "riscv64": "qemu-system-riscv64",
    }[arch]
    machine = "q35" if arch == "x86_64" else "virt"
    command = [binary, "-machine", machine, "-m", memory_mib, "-smp", cpus,
               "-display", "none", "-serial", "stdio", "-monitor", "none", "-no-reboot",
               "-drive", f"if=pflash,format=raw,readonly=on,file={firmware}",
               "-cdrom", iso]
    if arch == "aarch64":
        command += ["-cpu", "max"]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    output = bytearray()
    deadline = time.monotonic() + 50
    try:
        while time.monotonic() < deadline:
            for key, _ in selector.select(timeout=0.5):
                chunk = key.fileobj.read1(4096)
                if chunk:
                    output.extend(chunk)
                    if b"BOOT_OK" in output:
                        print(output.decode(errors="replace"))
                        return 0
            if process.poll() is not None:
                break
        print(output.decode(errors="replace"), file=sys.stderr)
        print("QEMU boot test failed: BOOT_OK not seen", file=sys.stderr)
        return 1
    finally:
        selector.close()
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()


if __name__ == "__main__":
    sys.exit(main())
