#!/usr/bin/env python3
"""Render the checked-in Limine template from a Kconfig generated .config."""

import ast
import pathlib
import re
import sys


def configured_cmdline() -> str:
    config = pathlib.Path(".config")
    if not config.exists():
        config = pathlib.Path(".config-default")
    match = re.search(r'^CONFIG_KERNEL_CMDLINE=("(?:[^"\\]|\\.)*")$',
                      config.read_text(), re.MULTILINE)
    if match is None:
        raise SystemExit(f"{config}: CONFIG_KERNEL_CMDLINE is missing")
    value = ast.literal_eval(match.group(1))
    if any(ord(char) < 32 for char in value):
        raise SystemExit("kernel command line contains a control character")
    return value


def main() -> None:
    source, destination, kaslr = sys.argv[1:]
    if kaslr not in ("yes", "no"):
        raise SystemExit("KASLR must be yes or no")
    template = pathlib.Path(source).read_text()
    output = template.replace("@KASLR@", kaslr)
    output = output.replace("@KERNEL_CMDLINE@", configured_cmdline())
    pathlib.Path(destination).write_text(output)


if __name__ == "__main__":
    main()
