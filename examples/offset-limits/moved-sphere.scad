// Refused: `A + (ball + t)` is the offset translated by `t`, and only the
// radius follows the shape, so a ball off the origin cannot be placed.
minkowski() {
  cube([2, 2, 2], center = true);
  translate([3, 0, 0]) sphere(0.5);
}
