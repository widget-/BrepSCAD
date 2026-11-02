// Refused: one sphere can round one solid, not two.
minkowski() {
  cube(1);
  translate([2, 0, 0]) cube(1);
  sphere(0.5);
}
