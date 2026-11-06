// Reduced OpenGrid snap: outer shell minus one horizontal and one vertical
// clip cutout. Their ends overlap inside the shell.
//
// The cutouts stop at a full cross-section rather than tapering to a 1e-6
// knife edge. A 1e-6-high slice while keeping the 0.4/0.2 chamfer makes the
// end cap a self-intersecting octagon: its outward side is undefined, the
// boolean cannot resolve the knife-edge overlap, and the shell comes out with
// boundary edges. OpenSCAD renders that input; this backend reports it.
$fn = 16;

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

module snap_shell() {
    // snap_outer_shell: 24.8 base, 6.8 high, 0.2 base chamfer, with the
    // corner chamfer stepping from 4.81838 to 3.25588 near the top.
    sweep([
        [0, 24.4, 24.4, 4.81838],
        [0.2, 24.8, 24.8, 4.81838],
        [4.8375, 24.8, 24.8, 4.81838],
        [6.4, 24.8, 24.8, 3.25588],
        [6.8, 24.8, 24.8, 3.25588]
    ]);
}

module horizontal_cutout() {
    translate([0, -12.5, 5.8])
    rotate([-90, 0, 0])
    sweep([
        [0, 12, 0.4, 0.4 / 3],
        [1.3, 12, 0.4, 0.4 / 3]
    ]);
}

module vertical_cutout() {
    translate([0, -11.1, -0.2])
    sweep([
        [0, 12, 0.6, 0.2],
        [5.8, 12, 0.6, 0.2]
    ]);
}

render()
difference() {
    snap_shell();
    horizontal_cutout();
    vertical_cutout();
}
