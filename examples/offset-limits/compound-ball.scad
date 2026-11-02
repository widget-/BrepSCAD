// Falls back to the sampled hull: the operand is a union of three balls, not
// one ball, which is the shape BOSL2's `cuboid(rounding=)` sums. Faceted by the
// operand's own `$fn`, and taken through tessellation rather than the offset.
minkowski() {
  cube([6, 6, 2], center = true);
  union() {
    sphere(0.5, $fn = 16);
    translate([1, 0, 0]) sphere(0.5, $fn = 16);
    translate([-1, 0, 0]) sphere(0.5, $fn = 16);
  }
}
