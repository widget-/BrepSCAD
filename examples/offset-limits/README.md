# Offset limits, one file each

Five minimal cases for the rounded offset (`minkowski` with a sphere, backed by
`Kernel::offset`). Open any of them in the viewer, or run
`brepscad <file>` from the workspace root; the three refusals print the message
in the last column.

| File | What it shows | `brepscad <file>` |
| --- | --- | --- |
| `rounded-cube.scad` | The exact offset: six plates, twelve quarter-cylinder sectors, eight sphere octants | `meshes: 1` (1100 triangles); seams up to 9e-3 wide, see below |
| `moved-sphere.scad` | A ball away from the origin cannot be placed, since only its radius follows the shape | `bad arguments for minkowski: the sphere must be centred at the origin` |
| `sphere-with-two-solids.scad` | One sphere rounds one solid, not two | `bad arguments for minkowski: needs exactly one sphere and one other solid` |
| `compound-ball.scad` | A union of balls is not one ball, which is the shape BOSL2's `cuboid(rounding=)` sums | `meshes: 1` (452 triangles) — the sampled hull, faceted by `$fn` |
| `cylinder-and-sphere.scad` | The offset is built for boxes only | `bad arguments for minkowski: cannot offset that shape: … offset of a shape with curved faces` |

The first and fourth both render; the difference is that the first is analytic
and exact, while the fourth is a convex hull of sampled points. The hairline
cracks in the first are the one visual cost and are described in
`docs/notes/2026-11-01.md`.
