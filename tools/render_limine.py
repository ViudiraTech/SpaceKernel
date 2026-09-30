#!/usr/bin/env python3
#
#       tools/render_limine.py
#       Render the checked-in Limine template from a Kconfig generated .config
#
#       2026/9/30 By JiTianYu391
#       Copyright (C) 2026 ViudiraTech.
#

"""Render the checked-in Limine template from a Kconfig generated .config."""

import ast
import os
import pathlib
import re
import sys


def configured_cmdline() -> str:
    if "CMDLINE" in os.environ and os.environ["CMDLINE"]:
        return os.environ["CMDLINE"]
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
    source = sys.argv[1]
    destination = sys.argv[2]
    kaslr = sys.argv[3]
    dtb_line = sys.argv[4] if len(sys.argv) > 4 else ""
    if kaslr not in ("yes", "no"):
        raise SystemExit("KASLR must be yes or no")
    template = pathlib.Path(source).read_text()
    output = template.replace("@KASLR@", kaslr)
    output = output.replace("@KERNEL_CMDLINE@", configured_cmdline())
    output = output.replace("@DTB_LINE@", dtb_line)
    pathlib.Path(destination).write_text(output)


if __name__ == "__main__":
    main()
