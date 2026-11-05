#!/usr/bin/env python3
"""Extract the string pool from Godot 4 compiled GDScript (.gdc) — no decompiler needed.

Why: there is **no** GDScript decompiler on this machine (no gdsdecomp/GDRE, no pip
package, and we must not download one). See docs/92 §2. But a `.gdc` is a compressed
container whose payload is a tokenised bytecode stream that carries a plain string
pool: identifiers, member names, signal names and literals. Recovering that pool does
not recover statements, but it does tell us *which* names a script touches, which is
exactly what a UI inventory needs (e.g. "which node does this script connect to?").

Container format (Godot 4.x, verified on 4.7.2 exports):

    offset 0   "GDSC"                      magic
    offset 4   u32                         bytecode version (101 here)
    offset 8   u32                         decompressed payload size
    offset 12  zstd frame                  payload

Payload: little-endian u32 token/operand stream; every string is
`u32 len` followed by `len` × { u8 c ^ 0xb6, 0xb6, 0xb6, 0xb6 } (4 bytes per char).

Usage:
    tools/gdc_strings.py rsc/ExitButtonContainer.gdc
    tools/gdc_strings.py --all rsc/            # one block per script
    tools/gdc_strings.py --grep ReplaysContainer --all rsc/
    tools/gdc_strings.py --json out.json --all rsc/

Exit code 2 with a message when zstandard is missing (the only dependency).
"""
import argparse
import json
import os
import struct
import sys

try:
    import zstandard
except ImportError:  # pragma: no cover - environment guard
    sys.exit("zstandard is required (pip install zstandard); nothing was downloaded")

KEY = 0xB6
MAGIC = b"GDSC"


def decode_container(raw: bytes) -> bytes:
    if raw[:4] != MAGIC:
        raise ValueError("not a .gdc (bad magic)")
    version, size = struct.unpack("<II", raw[4:12])
    payload = zstandard.ZstdDecompressor().decompress(raw[12:], max_output_size=64 << 20)
    if size and len(payload) != size:
        raise ValueError(f"size mismatch: header {size} decoded {len(payload)}")
    return payload


def extract_strings(payload: bytes) -> list:
    """Scan the token stream for length-prefixed, XOR-obfuscated strings."""
    out = []
    i = 0
    end = len(payload)
    while i + 4 <= end:
        n = int.from_bytes(payload[i:i + 4], "little")
        if 1 <= n <= 300 and i + 4 + 4 * n <= end:
            s = "".join(chr(payload[i + 4 + 4 * j] ^ KEY) for j in range(n))
            if all(32 <= ord(c) < 127 for c in s):
                out.append(s)
                i += 4 + 4 * n
                continue
        i += 4
    return out


def dump(path: str) -> dict:
    raw = open(path, "rb").read()
    version, size = struct.unpack("<II", raw[4:12])
    payload = decode_container(raw)
    return {
        "path": path,
        "name": os.path.basename(path),
        "version": version,
        "packed_bytes": len(raw),
        "payload_bytes": len(payload),
        "strings": extract_strings(payload),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="*")
    ap.add_argument("--all", metavar="DIR", help="decode every *.gdc under DIR")
    ap.add_argument("--grep", metavar="SUBSTR", help="only show scripts containing SUBSTR")
    ap.add_argument("--json", metavar="OUT")
    a = ap.parse_args()

    targets = list(a.paths)
    if a.all:
        for root, _dirs, files in os.walk(a.all):
            for f in sorted(files):
                if f.endswith(".gdc"):
                    targets.append(os.path.join(root, f))
    if not targets:
        ap.error("no .gdc given")

    results = []
    for t in targets:
        try:
            results.append(dump(t))
        except Exception as e:  # keep going: one bad file must not hide the rest
            print(f"!! {t}: {e}", file=sys.stderr)

    if a.grep:
        results = [r for r in results if any(a.grep in s for s in r["strings"])]
    if a.json:
        with open(a.json, "w", encoding="utf-8") as fh:
            json.dump(results, fh, ensure_ascii=False, indent=1)
        print(f"wrote {a.json} ({len(results)} scripts)")
        return 0

    for r in results:
        print(f"== {r['name']}  v{r['version']}  packed={r['packed_bytes']} payload={r['payload_bytes']}")
        print("   " + " | ".join(r["strings"]))
    print(f"\n{len(results)} scripts, {sum(len(r['strings']) for r in results)} strings")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
