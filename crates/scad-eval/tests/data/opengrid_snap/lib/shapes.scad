/*
    lib/shapes.scad

    Shape utilities for opengrid-bin-generator.

    This file provides a small set of primitives (rectangular prisms, hollow
    prisms, and a stacked prism sweep) that are built using an explicit
    "sweep"/"extrude along path" approach via `polyhedron()`.

    The goal is to avoid `hull()`-based blending, which can produce coplanar
    artifacts and unpredictable behavior for hollow walls and solid floors.

    Public API modules:
      - rect_prism()
      - hollow_rect_prism()
      - hollowable_rect_prism()
      - prism_stack()
      - zchamfered_cube()
*/

include <../lib/distributors.scad>
include <../lib/math.scad>


// -----------------------------------------------------------------------------
// Utility helpers
// -----------------------------------------------------------------------------

// Clamp a 2D base size to avoid degenerate polyhedron faces.
function _clamp_base(base) = [ max(base[0], 1e-6), max(base[1], 1e-6) ];

// Returns a CCW polygon for a rectangular footprint with optional chamfer.
// The vertex count is stable for a given chamfer, which is essential for
// sweep-based polyhedron generation.
function _chamfered_rect_points(base, chamfer) =
    chamfer <= 0
        ? [
            [-base[0]/2, -base[1]/2],
            [ base[0]/2, -base[1]/2],
            [ base[0]/2,  base[1]/2],
            [-base[0]/2,  base[1]/2]
        ]
        : [
            [ base[0]/2 - chamfer, -base[1]/2],
            [ base[0]/2, -base[1]/2 + chamfer],
            [ base[0]/2,  base[1]/2 - chamfer],
            [ base[0]/2 - chamfer,  base[1]/2],
            [-base[0]/2 + chamfer,  base[1]/2],
            [-base[0]/2,  base[1]/2 - chamfer],
            [-base[0]/2, -base[1]/2 + chamfer],
            [-base[0]/2 + chamfer, -base[1]/2]
        ];

// Computes the base size at a given Z position along a stack of tapered
// segments. Each segment is [height, angle] (in degrees):
//   - height: how tall the segment is along Z
//   - angle: slope angle relative to vertical (0 = vertical walls, 45 = 45° taper)
//
// The footprint changes linearly within each segment. For a given angle, the
// half-width change per Z is `tan(angle)`, so the full width change is `2*tan(angle)`.
//
// This function is recursive: it consumes the first segment, then recurses into
// the remainder if z lies beyond the first segment's height.
function _base_size_at_z(z, base, segments) =
    len(segments) == 0 ? base :
    z <= 0
        ? [ base[0] + z * tan(segments[0][1]) * 2,
            base[1] + z * tan(segments[0][1]) * 2 ]
        : let (
            _h = segments[0][0],
            _a = segments[0][1],
            _next_base = [ base[0] + _h * tan(_a) * 2, base[1] + _h * tan(_a) * 2 ],
            _rest = len(segments) <= 1
                ? []
                : [ for (i = [1 : len(segments) - 1]) segments[i] ]
        )
        ( z < _h
            ? [ base[0] + z * tan(_a) * 2, base[1] + z * tan(_a) * 2 ]
            : _base_size_at_z(z - _h, _next_base, _rest)
        );

// Returns a running total (prefix sum) for a list of numeric values.
// E.g. [a, b, c] => [a, a+b, a+b+c]
function _prefix_sums(values) =
    len(values) == 0 ? [] :
    len(values) == 1 ? [values[0]] :
    let (
        _first = values[0],
        _rest = [ for (i = [1 : len(values) - 1]) values[i] ],
        _rest_sums = _prefix_sums(_rest)
    )
    concat([_first], [ for (r = _rest_sums) _first + r ]);

// Extracts per-segment vertical chamfer values.
//
// Each segment can optionally include a third element:
//   [height, angle, [start_zchamfer, end_zchamfer]]
//
// If a segment does not specify a chamfer list, it inherits the previous
// chamfer value.
//
// The segment can optionally specify two values: [start_zchamfer, end_zchamfer].
// Either value can be `undef` to inherit the previous effective value.
// If only one value is given, it applies to both start and end.
function _segment_zchamfers(segments, prev_chamfer) =
    len(segments) == 0 ? [prev_chamfer] :
    let (
        _seg = segments[0],
        _rest = len(segments) <= 1 ? [] : [ for (i = [1 : len(segments) - 1]) segments[i] ],
        // Safely handle the optional chamfer sublist. If it is `undef`, treat
        // it as missing.
        _info_raw = len(_seg) > 2 ? _seg[2] : [],
        _info = is_undef(_info_raw) ? [] : _info_raw,

        // Interpret `undef` as "inherit the previous value".
        _raw_start = len(_info) > 0 ? _info[0] : undef,
        _start = is_undef(_raw_start) ? prev_chamfer : _raw_start,

        // If end is missing/undef, use the start value.
        _raw_end = len(_info) > 1 ? _info[1] : _raw_start,
        _end = is_undef(_raw_end) ? _start : _raw_end
    )
    concat([_start], _segment_zchamfers(_rest, _end));

// Sweeps a sequence of 2D profiles along Z to build a single polyhedron.
// All profiles must have the same number of vertices and consistent winding.
module _sweep_polyhedron(profiles, zs, convexity=10) {
    _n = len(profiles);
    if (_n < 2) {
        echo("sweep_polyhedron requires at least 2 profiles; got", _n);
    } else {
        _points_per_profile = len(profiles[0]);
        for (i = [0 : _n - 1]) {
            if (len(profiles[i]) != _points_per_profile) {
                echo("sweep_polyhedron: profile vertex count mismatch at index", i,
                     "expected", _points_per_profile, "got", len(profiles[i]));
            }
        }

        // Flatten profiles into a list of 3D points (x,y,z).
        _points = [
            for (i = [0 : _n - 1])
                for (p = profiles[i])
                    [p[0], p[1], zs[i]]
        ];

        // Caps
        _bottom = reverse([ for (i = [0 : _points_per_profile - 1]) i ]);
        _top_start = (_n - 1) * _points_per_profile;
        _top = [ for (i = [0 : _points_per_profile - 1]) _top_start + i ];

        // Side faces (triangulated).
        _sides = concat(
            [ for (i = [0 : _n - 2]) for (j = [0 : _points_per_profile - 1])
                [ i*_points_per_profile + j,
                  i*_points_per_profile + (j + 1) % _points_per_profile,
                  (i + 1)*_points_per_profile + (j + 1) % _points_per_profile ]
            ],
            [ for (i = [0 : _n - 2]) for (j = [0 : _points_per_profile - 1])
                [ i*_points_per_profile + j,
                  (i + 1)*_points_per_profile + (j + 1) % _points_per_profile,
                  (i + 1)*_points_per_profile + j ]
            ]
        );

        // Build face list and ensure normals are outward-facing.
        _faces = [ for (f = concat([_bottom], [_top], _sides)) reverse(f) ];

        polyhedron(
            points=_points,
            faces=_faces,
            convexity=convexity
        );
    }
}

// -----------------------------------------------------------------------------
// Shape primitives
// -----------------------------------------------------------------------------

/** Chamfered square */
module zchamfered_cube(size=[0, 0, 0], chamfer=0) {
    // callers should never ask for zero or negative sizes; we warn here so
    // the failure is visible.  do **not** early-exit – let the invalid input
    // propagate so OpenSCAD itself reports the problem.
    if (size[0] <= 0 || size[1] <= 0 || size[2] <= 0) {
        echo("Warning: zchamfered_cube called with non-positive size", size, "chamfer", chamfer);
    }
    if (chamfer > 0) {
        hull() {
            for (x=[-1, 1], y=[-1, 1]) {
                translate([
                    x * max(0, (size[0]/2 - chamfer)),
                    y * max(0, (size[1]/2 - chamfer)),
                    0
                ])
                cylinder(r=chamfer, h=size[2], $fn=4, center=true);
            }
        }
    } else {
        cube(size=size, center=true);
    }
}

/**
    * Rectangular prism
    * - base: the size of the base of the prism
    * - angle: the angle of the slope of the sides, in degrees.
    * - height: the height of the prism
    * - zchamfer: the size of the chamfer on the vertical edges.
    * - extra_top/extra_bottom: additional extension along Z (useful for
    *   boolean subtraction/clipping).
    */
module rect_prism(
    base=[0,0],
    angle=0,
    height=1,
    zchamfer=0,
    extra_top=0,
    extra_bottom=0,
    extra_debug=""
) {
    _base = [
        base[0] - extra_bottom * tan(angle) * 2,
        base[1] - extra_bottom * tan(angle) * 2
    ];

    _top = [
        base[0] + height * tan(angle) * 2 + extra_top * tan(angle) * 2,
        base[1] + height * tan(angle) * 2 + extra_top * tan(angle) * 2
    ];

    if (_base[0] <= 0 || _base[1] <= 0) {
        echo("rect_prism: base plane non-positive", _base,
             "input base", base, "angle", angle,
             "extra_bottom", extra_bottom, "extra_top", extra_top);
    }
    if (_top[0] <= 0 || _top[1] <= 0) {
        echo("rect_prism: top plane non-positive", _top,
             "input base", base, "angle", angle,
             "extra_bottom", extra_bottom, "extra_top", extra_top);
    }

    _base_clamped = _clamp_base(_base);
    _top_clamped  = _clamp_base(_top);

    _base_poly = _chamfered_rect_points(_base_clamped, zchamfer);
    _top_poly  = _chamfered_rect_points(_top_clamped, zchamfer);

    _sweep_polyhedron(
        [_base_poly, _top_poly],
        [-extra_bottom, height + extra_top]
    );
}

/**
    * Hollow rectangular prism (outer shell - inner shell subtraction).
    * Solid floor support is implemented by starting the inner shell above the base.
    */
module hollow_rect_prism(
    base=[0,0],
    angle=0,
    height=1,
    wall_thickness=1,
    zchamfer=0,
    solid_floor=false,
    extra_top=0,
    extra_bottom=0,
    extra_debug=""
) {
    _solid_floor_height = min(height, (solid_floor == false ? 0 : (solid_floor == true ? wall_thickness : solid_floor)));

    difference() {
        rect_prism(
            base=base,
            angle=angle,
            height=height,
            zchamfer=zchamfer,
            extra_top=extra_top,
            extra_bottom=extra_bottom,
            extra_debug=str("[Outer prism] ", extra_debug)
        );

        _inner_base = [ base[0] - wall_thickness * 2, base[1] - wall_thickness * 2 ];
        if (_inner_base[0] <= 0 || _inner_base[1] <= 0) {
            echo("Warning: hollow_rect_prism inner base <=0", _inner_base,
                 "outer", base,
                 "wall_thickness", wall_thickness,
                 "solid_floor", _solid_floor_height,
                 "angle", angle);
        }

        if (_solid_floor_height > 0) {
            up(_solid_floor_height)
            rect_prism(
                base=_inner_base,
                angle=angle,
                height=height - _solid_floor_height,
                zchamfer=max(zchamfer - wall_thickness / sqrt(2), 0),
                extra_top=extra_top + 0.2,
                extra_bottom=extra_bottom + 0.2,
                extra_debug=str("[Inner prism for solid floor] ", extra_debug)
            );
        } else {
            rect_prism(
                base=_inner_base,
                angle=angle,
                height=height,
                zchamfer=max(zchamfer - wall_thickness / sqrt(2), 0),
                extra_top=extra_top + 0.2,
                extra_bottom=extra_bottom + 0.2,
                extra_debug=str("[Inner prism for hollow] ", extra_debug)
            );
        }
    }
}

/**
    * Hollowable rectangular prism.
    */
module hollowable_rect_prism(
    base=[0,0],
    angle=0,
    height=1,
    wall_thickness=1,
    zchamfer=0,
    solid_floor=false,
    hollow=false,
    extra_top=0,
    extra_bottom=0,
    extra_debug=""
) {
    if (hollow) {
        hollow_rect_prism(
            base=base,
            angle=angle,
            height=height,
            wall_thickness=wall_thickness,
            zchamfer=zchamfer,
            solid_floor=solid_floor,
            extra_top=extra_top,
            extra_bottom=extra_bottom,
            extra_debug=str("[Hollow prism] ", extra_debug)
        );
    } else {
        rect_prism(
            base=base,
            angle=angle,
            height=height,
            zchamfer=zchamfer,
            extra_top=extra_top,
            extra_bottom=extra_bottom,
            extra_debug=str("[Outer prism] ", extra_debug)
        );
    }
}

/**
    * Stack of prisms built as a single sweep.
    *
    * Each segment is [height, angle]. `angle` is the slope angle in degrees
    * (0 = vertical, positive = outward taper, negative = inward taper).
    *
    * Segments can optionally override the per-slice vertical chamfer using
    * a third element: [height, angle, [start_zchamfer, end_zchamfer]].
    *
    * Since OpenSCAD does not allow omitting list elements, use `undef` to
    * indicate "inherit the previous chamfer" (e.g. [undef, 2] or [1, undef]).
    *
    * The `zchamfer` parameter is the initial vertical edge chamfer, and it is
    * inherited by subsequent segments unless overridden.
    *
    * Example:
    *   prism_stack(
    *     base=[20,20],
    *     // 1st segment: 8mm tall, vertical walls, start chamfer 0.5, end chamfer 0.5
    *     // 2nd segment: 4mm tall, 10° taper, keep previous chamfer values
    *     segments=[[8, 0, [0.5, 0.5]], [4, 10]],
    *     wall_thickness=1,
    *     zchamfer=0.5,
    *     hollow=true,
    *     solid_floor=1
    *   );
    *
    * `base_chamfer` / `top_chamfer` add a sloped chamfer to the bottom/top
    * of the stack (by adjusting the first/last segment height).
    *
    * The stack is constructed as a single polyhedron from z-slices. If
    * `hollow` is true, an inner shell is subtracted using an inner profile
    * that is offset by `wall_thickness`.
    *
    * This module relies on a global `$overlap` value (set in the calling
    * scope) to slightly extend the inner shell so that it reliably subtracts
    * cleanly from the outer shell without z-fighting.
    */
module prism_stack(
    base=[0,0],
    segments=[[]],
    wall_thickness=1,
    zchamfer=0,
    base_chamfer=0,
    top_chamfer=0,
    solid_floor=false,
    hollow=true,
    extra_bottom=0,   // additional downward extension on first segment
    extra_debug=""
) {
    if (base[0] <= 0 || base[1] <= 0) {
        echo("prism_stack called with non-positive base", base,
             "segments", segments);
        if (extra_debug != "") {
            echo(str("Extra debug: ", extra_debug));
        }
    }

    if (hollow && (base[0] < wall_thickness*2 || base[1] < wall_thickness*2)) {
        echo("prism_stack: input base smaller than twice wall thickness", base,
             "wall_thickness", wall_thickness);
        if (extra_debug != "") {
            echo(str("Extra debug: ", extra_debug));
        }
    }

    // Outer segment definition includes base/top chamfers but also preserves
    // any per-segment zchamfer overrides (segments[i][2]).
    _segments_outer = concat(
        base_chamfer > 0 ? [[base_chamfer, 45, undef]] : [],
        [
            for (i = [0 : len(segments) - 1])
                [
                    segments[i][0]
                        - (i == 0 ? base_chamfer : 0)
                        - (i == len(segments) - 1 ? top_chamfer : 0),
                    segments[i][1],
                    len(segments[i]) > 2 ? segments[i][2] : undef
                ]
        ],
        top_chamfer > 0 ? [[top_chamfer, -45, undef]] : []
    );

    // Compute a per-slice vertical chamfer (zchamfer) profile, including any
    // per-segment overrides. The resulting list has length len(_segments_outer)+1.
    _outer_zchamfers = _segment_zchamfers(_segments_outer, zchamfer);

    // If we add an extra bottom slice, duplicate the first chamfer value so the
    // arrays remain aligned (one chamfer value per Z boundary).
    if (extra_bottom > 0) {
        _outer_zchamfers = concat([_outer_zchamfers[0]], _outer_zchamfers);
    }

    // Inner chamfers are offset inward by the wall thickness (projected onto
    // the 45° chamfer direction), so the inner surface remains parallel.
    _inner_zchamfers_all = [ for (c = _outer_zchamfers) max(c - wall_thickness / sqrt(2), 0) ];

    // Inner segments should match the outer segment heights, but without the
    // base chamfer taper. This ensures the inner cutout maintains a consistent
    // wall thickness and does not “shrink” with the base chamfer.
    _segments_inner = concat(
        base_chamfer > 0 ? [[base_chamfer, 0]] : [],
        [
            for (i = [0 : len(segments) - 1])
                [
                    segments[i][0] - (i == 0 ? base_chamfer : 0)
                        - (i == len(segments) - 1 ? top_chamfer : 0),
                    segments[i][1]
                ]
        ]
    );

    // Convert segment definitions into useful derived values.
    // Each segment is [height, angle].
    _segment_heights = [ for (s = _segments_outer) s[0] ];
    _segment_tops = _prefix_sums(_segment_heights);  // Z coordinate at the top of each segment.

    _total_height = len(_segment_tops) == 0 ? 0 : _segment_tops[len(_segment_tops) - 1];

    _solid_floor =
        solid_floor == false ? 0 :
        solid_floor == true  ? min(_total_height, wall_thickness) :
        min(_total_height, solid_floor);

    // Outer shell Z positions (including optional downward extension). These are
    // the Z coordinates of each profile used for the sweep.
    _outer_zs = concat(
        extra_bottom > 0 ? [-extra_bottom] : [],
        [0],
        _segment_tops
    );

    // When a base chamfer exists, the bottom-most footprint is reduced by the
    // chamfer amount, then expands back to `base` over `base_chamfer` height.
    _outer_base_start = base_chamfer > 0
        ? [ base[0] - base_chamfer*2, base[1] - base_chamfer*2 ]
        : base;

    // Inner cutout should not follow the base chamfer; it should be based on the
    // original `base` and wall thickness.
    _inner_base_start = [
        base[0] - wall_thickness*2,
        base[1] - wall_thickness*2
    ];

    if (_outer_base_start[0] <= 0 || _outer_base_start[1] <= 0) {
        echo("prism_stack: outer base (with chamfer) is non-positive", _outer_base_start,
             "base", base, "base_chamfer", base_chamfer);
    }
    if (hollow && (_inner_base_start[0] <= 0 || _inner_base_start[1] <= 0)) {
        echo("prism_stack: inner base (with chamfer/wall_thickness) is non-positive", _inner_base_start,
             "base", base, "wall_thickness", wall_thickness, "base_chamfer", base_chamfer);
    }

    _outer_profiles = [
        for (i = [0 : len(_outer_zs) - 1])
            _chamfered_rect_points(
                _clamp_base(_base_size_at_z(_outer_zs[i], _outer_base_start, _segments_outer)),
                _outer_zchamfers[i]
            )
    ];

    // Debug: expose the generated outer profile vertices and computed base sizes.
    // This helps confirm that the outer profile at the main body matches the
    // requested `base` size.
    _outer_base_sizes = [ for (z = _outer_zs) _base_size_at_z(z, _outer_base_start, _segments_outer) ];
    _outer_last_index = len(_outer_profiles) - 1;

    // Determine which slice represents the start of the main body (after base
    // chamfer). This is where the outer footprint should equal `base`.
    _outer_body_index = base_chamfer > 0 ? 1 : 0;
    _outer_body_size = _outer_base_sizes[_outer_body_index];

    _outer_base_deltas = [ _outer_body_size[0] - base[0], _outer_body_size[1] - base[1] ];
    if (extra_debug != "" && max(abs(_outer_base_deltas[0]), abs(_outer_base_deltas[1])) > 1e-6) {
        echo("prism_stack: computed outer body size differs from requested base", base,
             "computed", _outer_body_size, "delta", _outer_base_deltas);
    }

    if (extra_debug != "") {
        echo("prism_stack debug:", extra_debug);
        echo("  base", base, "wall_thickness", wall_thickness, "segments", segments);
        echo("  outer_zs", _outer_zs);
        echo("  outer_base_sizes", _outer_base_sizes);
        echo("  outer_profile[0] (z=0)", _outer_profiles[0]);
        echo("  outer_profile[last] (z", _outer_zs[_outer_last_index], ")", _outer_profiles[_outer_last_index]);
    }

    if (!hollow || wall_thickness <= 0) {
        _sweep_polyhedron(_outer_profiles, _outer_zs);
    } else {
        if (_inner_base_start[0] <= 0 || _inner_base_start[1] <= 0) {
            echo("prism_stack: inner base (start) <= 0 (hollow not possible)", _inner_base_start,
                 "base", base, "wall_thickness", wall_thickness, "base_chamfer", base_chamfer);
            _sweep_polyhedron(_outer_profiles, _outer_zs);
        } else {
            _inner_zchamfer = max(zchamfer - wall_thickness / sqrt(2), 0);
            _overlap = $overlap;

            _inner_start = (_solid_floor > 0 ? _solid_floor : 0) - _overlap;
            _inner_end   = _total_height + _overlap;

            _inner_zs = concat(
                [_inner_start],
                [ for (i = [0 : len(_outer_zs) - 1])
                    if (_outer_zs[i] > _inner_start && _outer_zs[i] < _inner_end)
                        _outer_zs[i]
                ],
                [_inner_end]
            );

            _inner_zchamfers = concat(
                [_inner_zchamfers_all[0]],
                [ for (i = [0 : len(_outer_zs) - 1])
                    if (_outer_zs[i] > _inner_start && _outer_zs[i] < _inner_end)
                        _inner_zchamfers_all[i]
                ],
                [_inner_zchamfers_all[len(_inner_zchamfers_all) - 1]]
            );

            _inner_profiles = [
                for (i = [0 : len(_inner_zs) - 1])
                    _chamfered_rect_points(
                        _clamp_base(_base_size_at_z(_inner_zs[i], _inner_base_start, _segments_inner)),
                        _inner_zchamfers[i]
                    )
            ];

            difference() {
                _sweep_polyhedron(_outer_profiles, _outer_zs);
                _sweep_polyhedron(_inner_profiles, _inner_zs);
            }
        }
    }
}
