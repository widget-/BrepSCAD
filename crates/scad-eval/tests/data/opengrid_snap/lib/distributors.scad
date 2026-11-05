module up(distance) {
    translate([0, 0, distance]) children();
}
module down(distance) {
    translate([0, 0, -distance]) children();
}
module left(distance) {
    translate([-distance, 0, 0]) children();
}
module right(distance) {
    translate([distance, 0, 0]) children();
}
module fwd(distance) {
    translate([0, -distance, 0]) children();
}
module back(distance) {
    translate([0, distance, 0]) children();
}

module zrot_copies(angles=[], offset=0) {
    if (is_list(offset)) {
        for (i = [0 : len(angles) - 1]) {
            translate([0, 0, offset[i]]) {
                rotate([0, 0, angles[i]]) children();
            }
        }
    } else {
        translate([0, 0, offset]) {
            for (angle = angles) {
                rotate([0, 0, angle]) children();
            }
        }
    }
}