#!/usr/bin/env python3
#
#       tools/check_no_simd.py
#       Reject FP/SIMD instructions outside the explicit architecture state backend
#
#       2026/9/30 By JiTianYu391
#       Copyright (C) 2026 ViudiraTech.
#

"""Reject FP/SIMD instructions outside the explicit architecture state backend."""

import argparse
import re
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=["x86_64", "aarch64", "riscv64"])
    parser.add_argument("kernel")
    args = parser.parse_args()
    objdump = next(
        (path for name in ("llvm-objdump", "llvm-objdump-21", "llvm-objdump-19")
         if (path := shutil.which(name))),
        None,
    )
    if objdump is None:
        parser.error("LLVM objdump is required for all-architecture instruction audit")
    result = subprocess.run(
        [objdump, "-d", "--no-show-raw-insn", "--demangle", args.kernel],
        check=True, text=True, capture_output=True,
    )
    register = {
        "x86_64": re.compile(r"\b(?:[xyz]mm\d+|mm[0-7]|k[0-7]|st)\b"),
        "aarch64": re.compile(r"\b(?:[qvdsbh](?:[12]?\d|3[01])|fpcr|fpsr)\b"),
        "riscv64": re.compile(
            r"\b(?:f[ast]\d+|f\d+|v\d+|fcsr|fflags|frm|vstart|vcsr|vxrm|vxsat)\b"
        ),
    }[args.arch]
    function = ""
    failures = []
    for line in result.stdout.splitlines():
        symbol = re.match(r"^[0-9a-f]+ <(.+)>:$", line)
        if symbol:
            function = symbol.group(1)
            continue
        instruction = re.match(r"^\s*[0-9a-f]+:\s+(\S+)(.*)$", line)
        if instruction is None:
            continue
        mnemonic, operands = instruction.groups()
        # LLVM prints some system-register names in upper case (FPCR/FPSR).
        mnemonic, operands = mnemonic.lower(), operands.lower()
        allowed = (
            f"spacekernel::arch::{args.arch}::fpu::" in function
            or function.startswith((
                "spacekernel_fpsimd_", "spacekernel_riscv_f_", "spacekernel_riscv_d_",
            ))
        )
        fp = bool(register.search(operands.split("//")[0].split("<")[0]))
        if args.arch == "x86_64":
            # x87 also has implicit-register instructions such as fld1/fsin.
            fp |= mnemonic.startswith(("f", "xsave", "xrstor")) or mnemonic in (
                "emms", "ldmxcsr", "stmxcsr", "vzeroupper", "vzeroall",
            )
        elif args.arch == "riscv64":
            # vsetvli uses integer operands, while CSR aliases may omit names.
            fp |= mnemonic.startswith("v") or mnemonic in (
                "frcsr", "fscsr", "frflags", "fsflags", "frrm", "fsrm",
                "fsflagsi", "fsrmi",
            )
        if fp and not allowed:
            failures.append(f"{function}: {line.strip()}")
    if failures:
        print("Unexpected FP/SIMD instructions:\n" + "\n".join(failures[:30]))
        return 1
    print(f"{args.arch}: ordinary kernel code contains no FP/SIMD instructions")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
