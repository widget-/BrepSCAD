// Copied from the OpenGrid bin generator's modules/snap.scad as a regression
// fixture. The author relicensed this copy BSD-3-Clause to match this
// repository; see ../README.md.
include <../lib/shapes.scad>
include <../lib/distributors.scad>
include <../lib/transforms.scad>

include <../lib/constants.scad>

module snap_outer_shell(
  snap_thickness = 6.8,
  wall_thickness = 1,
  hollow_snaps = true,
) {
  prism_stack(
    base=[snap_body_width, snap_body_width],
    segments=[
      [snap_thickness - corner_wing_thickness - corner_wing_outer_thickness, 0],
      [corner_wing_thickness, 0, [undef, corner_wing_chamfer]],
      [corner_wing_outer_thickness, 0],
    ],
    wall_thickness=back_cut_offset_to_edge,
    zchamfer=snap_body_chamfer_width,
    base_chamfer=snap_bottom_edge_chamfer,
    top_chamfer=0,
    hollow=hollow_snaps,
    solid_floor=false
  );
}

module snap_inner_shell(
  snap_thickness = 6.8,
  wall_thickness = 1,
  hollow_snaps = true,
) {
  _hollow_base_width = snap_body_width - back_cut_offset_to_edge * 2 - back_cutout_thickness * 2;
  _outer_shell_connection_point = (snap_thickness - edge_clip_distance_from_top);
  // first segment is vertical, second connects the two shells at the top with a 45 degree outward angle
  _first_segment = _outer_shell_connection_point - (snap_body_width - _hollow_base_width) / 2;
  _second_segment = (snap_body_width - _hollow_base_width) / 2;
  up(wall_thickness)
    prism_stack(
      base=[_hollow_base_width - wall_thickness * 2, _hollow_base_width - wall_thickness * 2],
      segments=[
        [_first_segment, 0],
        [snap_thickness - _first_segment - _second_segment - wall_thickness * sqrt(2), 0],
        [_second_segment + wall_thickness * sqrt(2), 45, [undef, corner_wing_chamfer]],
      ],
      wall_thickness=wall_thickness,
      zchamfer=snap_body_chamfer_width - (snap_body_width - _hollow_base_width),
      base_chamfer=snap_bottom_edge_chamfer,
      top_chamfer=0,
      hollow=false
    );
}

/**
    The connector plate to attach to OpenGrid grids or other bins.

    Generally, you wouldn't use this part directly but rather as part of a grid that
    fits your intended part body.

    Snaps will be top-aligned to the XY plane, such that your part should start at Z=0.

    * directional (bool, default false): whether to use rigid tabs on the Y+ direction.
    * hollow_snaps (bool, default false): whether to make the inner part of the snap hollow.
    * wall_thickness (number, default 1): thickness of the walls of the snap. Only relevant if hollow_snaps is true.
    * snap_thickness (number, default 6.8): overall thickness of the snap.
    * snap_clip_thickness (number, default grid_clip_height): thickness of the clip part of the snap.
    * back_cut_offset_to_edge (number, default back_cut_offset_to_edge): the wall width of the notch in the clip.
                        Thicker walls will be sturdier but harder to insert and remove.
    * clips_enabled (bool, default true): whether to include the clip geometry and cutouts.
    * clips (bool[] as [X+, Y+, X-, Y-], default [true, true, true, true]): which clips to include.
*/
module snap(
  directional = false,
  hollow_snaps = false,
  wall_thickness = 1,
  snap_thickness = 8,
  snap_clip_thickness = grid_clip_height,
  back_cut_offset_to_edge = back_cut_offset_to_edge,
  clips_enabled = true,
  clips = [true, true, true, true]
) {
  _clip_angles = [
    for (
      x = [
        clips[0] ? 0 : undef,
        clips[1] ? 90 : undef,
        clips[2] ? 180 : undef,
        clips[3] ? 270 : undef,
      ]
    ) if (x != undef) x,
  ];
  down(snap_thickness) {
    difference() {
      union() {
        // main outer shell
        snap_outer_shell(
          snap_thickness=snap_thickness,
          wall_thickness=wall_thickness,
          hollow_snaps=false
        );

        if (clips_enabled) {
          zrot_copies(angles=_clip_angles) {
            _edge_clip_angled_segment_length = .6; // TODO
            fwd(snap_body_width / 2 - .1)
              up(snap_thickness - edge_clip_height - snap_clip_top_offset)
                prism_stack(
                  base=[edge_clip_width, 0.2],
                  segments=[
                    [_edge_clip_angled_segment_length, edge_clip_vertical_chamfer_angle],
                    [edge_clip_height - _edge_clip_angled_segment_length * 2, 0],
                    [_edge_clip_angled_segment_length, -edge_clip_vertical_chamfer_angle],
                  ],
                  zchamfer=0,
                  wall_thickness=0.05,
                  hollow=false
                );
          }
        }
      }
      // end union

      // since the clips require cutouts, there's an "inner" shell mostly just for hollow mode
      if (hollow_snaps) {
        snap_inner_shell(snap_thickness=snap_thickness, wall_thickness=wall_thickness, hollow_snaps=hollow_snaps);
      }

      if (clips_enabled) {
        zrot_copies(angles=_clip_angles) {
          // horizontal clip cutouts
          fwd(snap_body_width / 2 + .1)
            up(snap_thickness - cutout_distance_from_top - side_cutout_height / 2)
              rotate([-90, 0, 0])
                // zchamfered_cube(
                //     size=[cutout_width, side_cutout_height, back_cut_offset_to_edge + .1],
                //     chamfer=side_cutout_height/3
                // );
                prism_stack(
                  base=[cutout_width, side_cutout_height],
                  segments=[
                    [back_cut_offset_to_edge + back_cutout_thickness / 2, 0],
                    [side_cutout_height / 2, -45],
                  ],
                  zchamfer=side_cutout_height / 3,
                  hollow=false
                );

          // vertical clip cutouts
          fwd(snap_body_width / 2 - back_cut_offset_to_edge - back_cutout_thickness / 2)
            up(-side_cutout_height / 2)
              prism_stack(
                base=[cutout_width, back_cutout_thickness],
                segments=[
                  [snap_thickness - cutout_distance_from_top - side_cutout_height / 2, 0],
                  [back_cutout_thickness / 2, -45],
                ],
                zchamfer=back_cutout_thickness / 3,
                hollow=false
              );
        }
      }
    }
  }
}

module snap_grid(
  width = 1,
  length = 1,
  snap_thickness = 8,
  snap_clip_thickness = grid_clip_height,
  snap_clip_vertical_offset = 0,
  directional_top_row = false,
  outer_snap_clips_only = false,
  wall_thickness = 1,
  hollow_snaps = false
) {
  echo(outer_snap_clips_only=outer_snap_clips_only);
  right((width - 1) * grid_spacing / 2)
    back((length - 1) * grid_spacing / 2)for (row = [0:width - 1], col = [0:length - 1]) {
      _top_clip =
      // not enabled when it's the top row and directional is set.
      !(directional_top_row && (col == length - 1)) &&
      // but when directional is unset and outer_snap_clips_only is set, only enable for top row
      (!directional_top_row && outer_snap_clips_only && (col == length - 1)) ||
      // otherwise, enable for all except maybe the top row
      (!outer_snap_clips_only && !(directional_top_row && col == length - 1));

      echo(
        row=row, col=col,
        check1=!(directional_top_row && (col == length - 1)),
        check2=(!directional_top_row && outer_snap_clips_only && (col == length - 1)),
        check3=(!outer_snap_clips_only && !(directional_top_row && col == length - 1)),
        top_clip=_top_clip
      );

      _clips = [
        _top_clip,
        outer_snap_clips_only ? (row == 0) : true,
        outer_snap_clips_only ? (col == 0) : true,
        outer_snap_clips_only ? (row == width - 1) : true,
      ];
      left(row * grid_spacing)
        fwd(col * grid_spacing)
          snap(
            directional=directional_top_row && row == 0,
            hollow_snaps=hollow_snaps,
            clips_enabled=true,
            clips=_clips,
            wall_thickness=wall_thickness,
            snap_thickness=snap_thickness,
            snap_clip_thickness=snap_clip_thickness,
            back_cut_offset_to_edge=back_cut_offset_to_edge
          );
    }
}
