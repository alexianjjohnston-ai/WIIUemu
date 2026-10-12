#!/usr/bin/env python3
"""Generate crates/wiiu-tracking/src/tag36h11.rs from AprilRobotics/apriltag's tag36h11.c.

Usage: python3 scripts/gen_tag36h11.py <path/to/apriltag/tag36h11.c> crates/wiiu-tracking/src/tag36h11.rs
"""
import re
import sys


def main(src_path: str, out_path: str) -> None:
    src = open(src_path).read()
    codes = re.findall(r"0x([0-9a-f]{16})UL", src)
    bx = [int(v) for v in re.findall(r"bit_x\[\d+\] = (\d+);", src)]
    by = [int(v) for v in re.findall(r"bit_y\[\d+\] = (\d+);", src)]
    assert len(codes) == 587 and len(bx) == 36 and len(by) == 36, "unexpected tag36h11.c format"
    licence = src.split("*/")[0].lstrip("/*").strip()

    out = [
        "//! AprilTag `tag36h11` family data, generated from AprilRobotics/apriltag `tag36h11.c`",
        "//! (commit b7c0ebe9aa20, BSD 2-Clause). Regenerate with `scripts/gen_tag36h11.py`; do not edit by hand.",
    ]
    out += [("// " + line).rstrip() for line in licence.splitlines()]
    out += [
        "",
        "/// Minimum Hamming distance between codes, including rotations.",
        "pub const MIN_HAMMING: u32 = 11;",
        "pub const NBITS: usize = 36;",
        "/// Side of the black-bordered square the detector reports corners for, in cells.",
        "pub const WIDTH_AT_BORDER: usize = 8;",
        "/// Side including the 1-cell white quiet zone, in cells.",
        "pub const TOTAL_WIDTH: usize = 10;",
        "",
        "/// Cell column of code bit `i` (MSB first), relative to the black border's top-left cell.",
        "pub const BIT_X: [u8; NBITS] = [" + ", ".join(map(str, bx)) + "];",
        "/// Cell row of code bit `i` (MSB first), relative to the black border's top-left cell.",
        "pub const BIT_Y: [u8; NBITS] = [" + ", ".join(map(str, by)) + "];",
        "",
        "pub const CODES: [u64; 587] = [",
    ]
    fmt = lambda c: "0x" + "_".join(c[i : i + 4] for i in range(0, 16, 4)) + ","
    for i in range(0, len(codes), 4):
        out.append("    " + " ".join(fmt(c) for c in codes[i : i + 4]))
    out.append("];")
    open(out_path, "w").write("\n".join(out) + "\n")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
