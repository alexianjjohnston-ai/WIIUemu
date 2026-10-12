# Vendored AprilTag

Source: https://github.com/AprilRobotics/apriltag, commit `b7c0ebe9aa20f82ec7a828579004f9e706bfecd9`
(2026-08-07), BSD 2-Clause (`apriltag/LICENSE.md`). Copied unmodified:

- `apriltag.c`, `apriltag.h`, `apriltag_math.h`, `apriltag_quad_thresh.c`
- `tag36h11.c`, `tag36h11.h` (the only tag family Wii U Connect uses)
- `common/*` except `getopt.*` (command-line tools only)

To update: copy the same files from a newer upstream commit, update the commit above, and run
`cargo test -p wiiu-tracking --features detector`.
