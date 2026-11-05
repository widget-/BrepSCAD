LEFT = 90;
BACK = 180;
RIGHT = 270;
FRONT = 0;

/**
  * OpenGrid system units
  */

// Size of a tile in the OpenGrid system.
grid_spacing = 28;

// The thickness of the wall surrounding a snap. Total wall thickness is double this.
grid_wall_width = 1.5;

/**
  * Measurements
  *
  * These were taken from the provided base snap STEP file.
  *
  * Some are (close) approximations, such as the shape of the clip notch.
  */

// Tolerance to give between the snap grid and snap clips.
snap_to_grid_tolerance_offset = 0.1;

/**
  * Main body size measurements
  */

// 
snap_body_width = 25 - snap_to_grid_tolerance_offset * 2;

// 
snap_body_main_width = 15;

// The width of the outer chamfer. No matter what I try, I cannot figure out where this number comes from in the reference model.
snap_body_chamfer_width = 4.81838;

// Thickness of a standard snap.
snap_body_thickness_full = 6.8;

// Thickness of a "lite" snap.
snap_body_thickness_lite = 4;

// Thickness of an unofficial "half-standard" snap. This allows two snaps to fit on both sides of a standard-thickness grid.
snap_body_thickness_half_standard = snap_body_thickness_full / 2;

/**
  * Top corner "wing" measurements
  */

// The stubby end of the corner wing's vertical thickness.
corner_wing_outer_thickness = 0.4;

// The overall thickness of the corner wing.
corner_wing_thickness = (snap_body_chamfer_width - corner_wing_outer_thickness) / sqrt(2) / 2;

//
corner_wing_chamfer = snap_body_chamfer_width - corner_wing_thickness;

/**
  * Clip measurements
  *
  * The edge clips are a slightly-rounded chamfered shape in the original model.
  *
  * This project attempts to approximate them with only a chamfer, as large smooth fillets
  * can be expensive to render in OpenSCAD.
  */

// Width of the largest part of the clip
// edge_clip_width = 11 - snap_to_grid_tolerance_offset * 2;
// TODO implement edge clip chamfer and revert this
edge_clip_width = (11 - snap_to_grid_tolerance_offset * 2) / 2;

// Height of the largest part of the clip
edge_clip_height = 2;

// Depth of the clip. This is how far it will stick out from the snap.
edge_clip_depth = .5 - snap_to_grid_tolerance_offset;

// The distance between the top of the snap and the start of the top of the clip.
edge_clip_distance_from_top = 1.4;

// Chamfer angle of the top/bottom of the clip.
edge_clip_vertical_chamfer_angle = 35;

// Chamfer angle of the left/right of the clip.
edge_clip_horizontal_chamfer_angle = 80;

/**
  * Clip cutout measurements
  *
  * The clips have the area above and behind them removed to allow the flexing
  * of the print to act as a retention mechanism of the snap.
  *
  * The "back" cutout is the vertical one, starting from the base of the snap.
  *
  * The "side" cutout is the horizontal one, just above the clip.
  */

// Width of both the back and side cutouts.
cutout_width = 12;

// Distance between the top of the snap and the start of both cutouts.
cutout_distance_from_top = 0.8;

// Thickness of the back cutout.
back_cutout_thickness = 0.6;

// Distance from the side of the snap to the start of the back cutout.
back_cutout_distance_from_edge = 1 - back_cutout_thickness / 2;

// Height of the side cutout.
side_cutout_height = 0.4;

/**
  * Extra constants
  */
snap_bottom_edge_chamfer = 0.2;

snap_clip_edges_normal = [LEFT, FRONT, RIGHT, BACK];
snap_clip_edges_directional = [LEFT, FRONT, RIGHT];
snap_clip_wings = [LEFT+FRONT, RIGHT+FRONT, LEFT+BACK, RIGHT+BACK];

// distance from top of snap to top of clip
snap_clip_top_offset = 1.4;

// 12 light rainbow debug colors
_debug_colors = [
    "00FFFF",
    "4264ff",
    "ff43ff",
    "ff9e42",
    "b2ff42",
    "42c6ff",
    "b264ff",
    "ff5656",
    "ffe242",
    "42ff9e",
];