#!/usr/bin/env python3
"""Download a pinned release asset with verified HTTP range requests.

Each worker writes a disjoint range to a temporary file. The final file is
published only after every range and the complete SHA-256 digest are verified.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import os
from pathlib import Path
import shutil
import tempfile
import time
from urllib.request import Request, urlopen


def fetch_range(url: str, start: int, end: int, destination: Path, total_size: int) -> Path:
    expected = end - start + 1
    for attempt in range(3):
        try:
            request = Request(
                url,
                headers={"Range": f"bytes={start}-{end}", "Accept-Encoding": "identity"},
            )
            with urlopen(request, timeout=45) as response:
                if response.status != 206:
                    raise RuntimeError(f"range request returned HTTP {response.status}")
                if response.headers.get("Content-Range") != f"bytes {start}-{end}/{total_size}":
                    raise RuntimeError("server returned an unexpected Content-Range")
                with destination.open("wb") as output:
                    shutil.copyfileobj(response, output)
            if destination.stat().st_size != expected:
                raise RuntimeError("range response length mismatch")
            return destination
        except (OSError, RuntimeError):
            if attempt == 2:
                raise
            time.sleep(0.25 * (2 ** attempt))
    raise AssertionError("unreachable")


def fetch_single(url: str, destination: Path) -> Path:
    with urlopen(Request(url, headers={"Accept-Encoding": "identity"}), timeout=45) as response:
        if response.status != 200:
            raise RuntimeError(f"download returned HTTP {response.status}")
        with destination.open("wb") as output:
            shutil.copyfileobj(response, output)
    return destination


def download(url: str, destination: Path, expected_hash: str, workers: int) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    head = Request(url, method="HEAD", headers={"Accept-Encoding": "identity"})
    with urlopen(head, timeout=45) as response:
        size = int(response.headers.get("Content-Length", "0"))
        ranges = response.headers.get("Accept-Ranges", "").lower() == "bytes"
    if size <= 0:
        raise RuntimeError("server did not provide a positive Content-Length")

    with tempfile.TemporaryDirectory(prefix=".download-", dir=destination.parent) as temp_name:
        temp = Path(temp_name)
        if ranges and workers > 1:
            parts = min(workers, size)
            spans = [
                (index * size // parts, (index + 1) * size // parts - 1, temp / f"part-{index}")
                for index in range(parts)
            ]
            print(f"Downloading {size} bytes with {parts} parallel range requests", flush=True)
            with ThreadPoolExecutor(max_workers=parts) as executor:
                list(executor.map(lambda span: fetch_range(url, *span, size), spans))
            inputs = [span[2] for span in spans]
        else:
            print("Server has no byte-range support; using one connection", flush=True)
            inputs = [fetch_single(url, temp / "part-0")]

        combined = temp / "complete"
        digest = hashlib.sha256()
        with combined.open("wb") as output:
            for part in inputs:
                with part.open("rb") as source:
                    while chunk := source.read(1024 * 1024):
                        output.write(chunk)
                        digest.update(chunk)
        if combined.stat().st_size != size:
            raise RuntimeError("assembled download length mismatch")
        if digest.hexdigest().lower() != expected_hash.lower():
            raise RuntimeError("SHA-256 checksum mismatch")
        os.replace(combined, destination)
        print(f"Verified SHA-256: {digest.hexdigest()}", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("url")
    parser.add_argument("destination", type=Path)
    parser.add_argument("sha256")
    parser.add_argument("--jobs", type=int, default=8)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 32:
        parser.error("--jobs must be between 1 and 32")
    if len(args.sha256) != 64 or any(char not in "0123456789abcdefABCDEF" for char in args.sha256):
        parser.error("sha256 must be a 64-digit hexadecimal digest")
    download(args.url, args.destination, args.sha256, args.jobs)


if __name__ == "__main__":
    main()
