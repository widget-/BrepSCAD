// Regression fixture: OpenGrid snap with a single clip.
//
// Same parameters as the OpenGrid bin generator's opengrid-snap.scad, with
// `clips` reduced to one. This model used to overflow the evaluator's stack:
// the boolean engine recursed once per retry in its curve/surface projection,
// and the shell-set union recursed again when a fusion failed. One clip still
// produces both. See README.md.

include <./lib/constants.scad>
include <./modules/snap.scad>

hollow_snaps = true;
snap_thickness = 6.8;
snap_clip_thickness_fill = false;
snap_directional_top_row = true;
outer_snap_clips_only = "auto";
strong_clips = false;
wall_thickness = 1;
$slop = -0.05;
fast_preview = true;

back_cut_offset_to_edge = strong_clips ? 1.4 : 1;
snap_clip_thickness = snap_clip_thickness_fill
    ? (max(snap_thickness - 2.8, 2))
    : (snap_thickness == 6.8 ? 2 : 1.8);

snap_clip_vertical_offset = snap_clip_thickness_fill == "fill"
    ? 0
    : (snap_thickness - snap_clip_thickness) / 2 - snap_clip_top_offset;

bin_outer_chamfer = snap_body_chamfer_width;
bin_inner_chamfer = snap_body_chamfer_width - wall_thickness * 2 * sqrt(2);

$overlap = .02;

snap(
    directional=snap_directional_top_row,
    hollow_snaps=hollow_snaps,

    wall_thickness=wall_thickness,
    snap_thickness=snap_thickness,
    snap_clip_thickness=snap_clip_thickness,
    back_cut_offset_to_edge=back_cut_offset_to_edge,
    clips=[true, false, false, false]
);
