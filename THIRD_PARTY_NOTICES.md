# Third-party notices

## AprilTag (tag36h11 code table)

`crates/wiiu-tracking/src/tag36h11.rs` contains data generated from
[AprilRobotics/apriltag](https://github.com/AprilRobotics/apriltag) `tag36h11.c` (commit `b7c0ebe9aa20`).

> Copyright (C) 2013-2016, The Regents of The University of Michigan. All rights reserved.
> Licensed under the BSD 2-Clause License. The full notice is reproduced at the top of the generated file.

## AprilTag C library (PC-side detector)

`crates/wiiu-apriltag-sys/vendor/apriltag/` contains unmodified sources from AprilRobotics/apriltag (commit
`b7c0ebe9aa20`), BSD 2-Clause; the licence is at `crates/wiiu-apriltag-sys/vendor/apriltag/LICENSE.md`. It is compiled
into `wiiu-connect` for camera tracking (see `docs/adr/0001-apriltag-detection-on-pc.md`).

## Cemu

Wii U Connect does not include Cemu code. Packet layouts and profile formats were implemented from reading the
Cemu source (MPL-2.0) for compatibility; see `docs/research.md`.
