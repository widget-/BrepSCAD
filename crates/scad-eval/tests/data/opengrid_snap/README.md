# OpenGrid snap regression fixtures

Reduced models from the author's OpenGrid bin generator
(`opengrid-bin-generator/opengrid-snap.scad`, module sources copied under
`lib/` and `modules/`). The author relicensed this copy BSD-3-Clause to match
this repository.

`snap_one_clip.scad` and `snap_two_clips.scad` reproduce two unbounded
recursions that used to overflow an 8 MB stack:

- `monstertruck-solid`'s `curve_surface_projection` retried a fixed-point
  projection by recursing (100 levels). Fixed by iterating.
- `TruckKernel::union_shell_sets` merged shell pairs by re-entering the
  compound split, so a run of failed fusions recursed once per merge. Fixed by
  merging through `merge_solids`, which runs the engine directly.

History, measured with the engine as it stood before the fixes:

| fixture | binary state | result |
| --- | --- | --- |
| one clip | both recursions present | stack overflow |
| two clips | projection recursion fixed | stack overflow |
| one clip | only the merge recursion fixed | completes (the projection loop needs the stack the merge recursion consumed) |

`crates/scad-eval/tests/booleans.rs` evaluates both fixtures and checks the
mesh volume. The meshes are open at the clip cutouts, matching the standalone
snap model; only evaluation success is asserted, not watertightness.
