// Refused: the offset is built for boxes, so a cylinder has no rounded form yet.
minkowski() {
  cylinder(r = 1, h = 2, center = true);
  sphere(0.25);
}
