// Exact offset of a box by a ball. Renders, but the patch seams show as
// hairlines: the plates, edge cylinders and corner octants do not share
// topology, so the display mesh has hairline cracks (128 open edges here).
minkowski() {
  cube([2, 2, 2], center = true);
  sphere(0.5);
}
