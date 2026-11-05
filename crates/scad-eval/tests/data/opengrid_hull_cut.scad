// Reduced 2x2 OpenGrid bin: outer - cells - hull, built from explicit slices.
// The hull's bottom plane is exactly at the cell cutters' 45-degree crease.
$fn = 16;

cw = 4.81838 - (4.81838 - 0.4) / sqrt(2) / 2;
cell_c = cw - 0.25;
hull_c = cw - 1;
outer_center = 56 - 1 - cw * sqrt(2);
hull_base = 56 - 2 - cw * 2 / sqrt(3);

function rev(v) = [for (i = [0 : len(v) - 1]) v[len(v) - 1 - i]];

function crect(base, chamfer) = [
    [ base[0]/2 - chamfer, -base[1]/2],
    [ base[0]/2, -base[1]/2 + chamfer],
    [ base[0]/2,  base[1]/2 - chamfer],
    [ base[0]/2 - chamfer,  base[1]/2],
    [-base[0]/2 + chamfer,  base[1]/2],
    [-base[0]/2,  base[1]/2 - chamfer],
    [-base[0]/2, -base[1]/2 + chamfer],
    [-base[0]/2 + chamfer, -base[1]/2]
];

module sweep(slices) {
    n = len(slices);
    pp = 8;
    points = [
        for (s = slices)
            for (p = crect([s[1], s[2]], s[3]))
                [p[0], p[1], s[0]]
    ];
    bottom = rev([for (i = [0 : pp - 1]) i]);
    top_start = (n - 1) * pp;
    top = [for (i = [0 : pp - 1]) top_start + i];
    sides = concat(
        [for (i = [0 : n - 2]) for (j = [0 : pp - 1])
            [i*pp + j, i*pp + (j + 1) % pp, (i + 1)*pp + (j + 1) % pp]],
        [for (i = [0 : n - 2]) for (j = [0 : pp - 1])
            [i*pp + j, (i + 1)*pp + (j + 1) % pp, (i + 1)*pp + j]]
    );
    faces = [for (f = concat([bottom], [top], sides)) rev(f)];
    polyhedron(points = points, faces = faces, convexity = 10);
}

module corner_prism() {
    sweep([[0, 24.8, 24.8, cw], [1.6, 28, 28, cw], [101, 28, 28, cw]]);
}

module center_prism() {
    sweep([
        [1, outer_center, outer_center, cw],
        [2, outer_center + 2, outer_center + 2, cw],
        [101, outer_center + 2, outer_center + 2, cw]
    ]);
}

module outer() {
    translate([14, 14, 0]) corner_prism();
    translate([14, -14, 0]) corner_prism();
    translate([-14, 14, 0]) corner_prism();
    translate([-14, -14, 0]) corner_prism();
    center_prism();
}

module cell() {
    sweep([
        [0, 22.8, 22.8, cell_c],
        [1.6, 26, 26, cell_c],
        [97.6, 26, 26, cell_c],
        [98.2, 24.8, 24.8, cell_c],
        [100, 24.8, 24.8, cell_c],
        [101.6, 28, 28, cell_c],
        [102.6, 28, 28, cell_c]
    ]);
}

module cells() {
    translate([14, 14, 0]) cell();
    translate([14, -14, 0]) cell();
    translate([-14, 14, 0]) cell();
    translate([-14, -14, 0]) cell();
}

module hullcut() {
    translate([0, 0, 1.6])
    sweep([
        [0, hull_base, hull_base, hull_c],
        [96, hull_base, hull_base, hull_c],
        [96.6, hull_base - 1.2, hull_base - 1.2, hull_c],
        [98.4, hull_base - 1.2, hull_base - 1.2, hull_c],
        [100, hull_base + 0.4, hull_base + 0.4, hull_c],
        [101, hull_base + 0.4, hull_base + 0.4, hull_c]
    ]);
}

render()
difference() {
    difference() {
        outer();
        cells();
    }
    hullcut();
}
