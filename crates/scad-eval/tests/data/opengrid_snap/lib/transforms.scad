include <./distributors.scad>

module half(direction=[0, 0, 1], size=1000) {
    difference() {
        union() children();

        translate(direction * size / 2)
        cube(size, center=true);
    }
}

module back_half(size=1000) {
    half(direction=[0, -1, 0], size=size) {
        children();
    }
}

module front_half(size=1000) {
    half(direction=[0, 1, 0], size=size) {
        children();
    }
}

module left_half(size=1000) {
    half(direction=[1, 0, 0], size=size) {
        children();
    }
}

module right_half(size=1000) {
    half(direction=[-1, 0, 0], size=size) {children();}
}

module top_half(size=1000) {
    half(direction=[0, 0, -1], size=size) {
        children();
    }
}

module bottom_half(size=1000) {
    half(direction=[0, 0, 1], size=size) {
        children();
    }
}