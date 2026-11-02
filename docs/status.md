# Project status

This file tracks the current state. Dated snapshots live in
[`notes/`](notes/).

## What this project is

A B-rep-first visual editor for OpenSCAD-style `.scad` source. The `.scad`
text is the canonical document. The editor should present a source-shaped tree,
write edits back into the source immediately, and keep inspection state
(such as showing one intermediate state) session-only.

BrepSCAD is an independent project and is not affiliated with OpenSCAD.

Exact OpenSCAD output identity is explicitly out of scope. Users who need that
should use OpenSCAD itself. Manifold is not a selectable backend; at most it
could be an internal mesh fallback for inherently mesh operations such as
`hull`, `minkowski`, and mesh imports.

## Current workspace

```text
crates/
  cli/          CLI entry point (package `brepscad`)
  kernel-api/   kernel trait, ShapeId, capabilities, neutral mesh, matrix transform
  kernel-truck/ Truck B-rep adapter
  raycast/      analytic ray casting over trimmed B-rep faces
  scad-eval/    evaluator and operand/parameter provenance
  scad-syntax/  lossless rowan CST, lexer, parser, text edits
  viewer/       eframe/egui/wgpu viewer
assets/
  sample.scad   built-in cube-with-hole source
docs/
  status.md     current capabilities and next steps
  development.md development helpers, headless UI snapshots
  notes/        dated session snapshots
  research.md   prior art, source-first architecture, kernel trade-offs
  kernels.md    OCCT / SolidCore / Truck capability matrix
  editor-lsp-plan.md editor and language-server plan
examples/
  offset-limits/ minimal scenes for the rounded offset's known limits
scripts/
  fetch-scad-corpus.sh   fetch the pinned BOSL2 + MCAD corpus
  check-scad-corpus.sh   compatibility runs over a .scad tree
  screenshot-viewer.sh   headless UI snapshots
vendor/
  README.md     which Truck crates are vendored and what changed in each
  truck-*/      Truck master stack, patched for boolean robustness
```

## What works

### Source layer

- Lossless rowan CST for a small OpenSCAD subset.
- Trivia-preserving lexer.
- Tolerant recursive-descent parser with error recovery.
- `TextEdit` / `apply_edit` with bounds and UTF-8 boundary checks.
- Supported syntax: top-level assignments, module and function declarations
  with parameters and defaults, module calls, function-call expressions,
  positional and named arguments, numbers (including negative literals,
  leading-dot forms, and exponents), string literals, identifiers (including
  digit-leading names), vectors, booleans, `undef`, ranges, comments,
  whitespace, trailing commas, and empty statements.
- `include` / `use` statements accept quoted or angle-bracket paths and an
  optional semicolon.
- Expressions support `+`, `-`, `*`, `/`, `%`, `^`, comparisons, `&&`, `||`,
  `!`, ranges, ternary `? :`, call expressions, postfix indexing, dot notation,
  `let`, list comprehensions (including C-style `for`), `each`, unary signs, and
  parentheses; the tree keeps every token.
- `for` / `if` / `else` statements, including blocks and single-statement bodies.
- `echo` / `assert` are recognized as both module calls and expression prefixes;
  `*`, `#`, `%`, and `!` statement modifiers are preserved in the tree.
- Transform and boolean calls support either a block or a single child module
  call.
- `module` declarations support block or single-statement bodies; `function`
  declarations use expression bodies.

### Evaluator

- Top-level assignments and variable lookup.
- User modules: declarations are collected per block, so a module can be called
  before its declaration in the same block. Parameters bind positionally or by
  name, defaults can reference earlier parameters, and calls can recurse.
- User functions: same declaration and binding rules, with expression bodies.
  Functions can recurse, call other functions, and resolve normal variables in
  their defining scope. Function literals (`function(x) ...`) are first-class
  values: they capture their defining scope, can be passed, returned, stored in
  lists, and invoked with `f(...)` or chained calls such as `f(2)(3)`.
  `is_function` recognizes them. Conditional expressions (`? :`) evaluate only
  the selected branch.
- Built-in functions: `abs`, `sign`, `sqrt`, `ln`, `log`, `exp`, `floor`,
  `ceil`, `round`, the degree-based trig functions, the hyperbolic functions,
  `atan2`, `pow`, `min`, `max`, `norm`, `cross`, `len`, `concat`, `str`, `chr`,
  `ord`, `lookup`, `search`, `rands`, `is_num`, `is_nan`, `is_bool`, `is_list`,
  `is_string`, `is_undef`, `is_function`, `version`, and `version_num`.
  `chr` accepts a number or a list of code points, `lookup` interpolates
  between numeric table keys and clamps outside the table, `cross` accepts 2D
  vectors and returns a scalar, `rands` accepts a `seed_value` named argument,
  and `min`/`max` return `undef` for empty or non-numeric inputs.
- `undef` is a value: it is false in conditions, propagates through arithmetic,
  compares equal to itself, and is detected by `is_undef`. Strings, vectors,
  and matrices follow OpenSCAD truthiness (empty is false).
- Unknown variables and omitted module/function arguments without defaults
  evaluate to `undef`, matching OpenSCAD.
- String values support escapes (including `\x`, `\u`, and `\U`), equality,
  `len`, character indexing, and `str` formatting.
- `echo(...)` records messages on `Evaluation::messages`; it can prefix an
  expression, act as a module statement, and render its children. `assert(...)`
  takes the same forms and aborts evaluation with `EvalError::Assertion` when
  its condition is false.
- `*` disables a modified subtree. `#`, `%`, and `!` evaluate normally for now;
  their highlight, background, and root-only presentation semantics are not yet
  implemented.
- `include` evaluates another source file in place, sharing assignments and
  definitions. Included-file assignments are applied in source order before the
  including file's own assignments, so root constants can reference included
  constants. `use` imports only module and function definitions from the used
  file's own hidden scope, so used library code does not see caller variables or
  overrides. Quoted paths resolve relative to the including file; angle-bracket
  paths also search `OPENSCADPATH` and `--library-path` directories. Cycles and
  read/parse failures are reported with the file path.
- Lists are heterogeneous and arbitrarily nested: numbers, strings, booleans,
  lists, and `undef` can be mixed. They are the only collection type; matrices
  are lists of lists.
- Postfix indexing reads list elements, nested rows, and string characters:
  `v[2]`, `m[1][0]`, `"abc"[1]`; invalid indexes and unknown dot members are
  `undef`. Dot notation reads `.x`, `.y`, and `.z`.
- `let (a = ..., b = ...) expr` binds sequential locals for one expression;
  `let (...) statement` does the same for a module body.
- List comprehensions support multiple bindings, nesting, `if` filters with
  optional `else`, `let`, `each`, and C-style `for (init; condition; next)`.
  Nested `for` comprehensions flatten.
- Assignments are hoisted to the start of their file or block. The last
  assignment to a name wins, evaluated at the position of its first definition;
  forward references and self-references yield `undef`, matching OpenSCAD.
- Module and function declarations are hoisted across `include` boundaries, so
  an included file can use a declaration from a file included later.
- Module and function calls use persistent environments. Normal variables and
  declarations resolve through the lexical parent (the definition site); `$`
  variables resolve through the dynamic parent chain, matching OpenSCAD.
  Module and loop scopes shadow and restore their enclosing bindings.
- `children()` renders all call-site children; `children(n)` selects one index
  and `children([...])` selects a range or list of indices. Children resolve
  normal variables in the caller's scope but `$` variables through the module
  body's dynamic scope, so values such as `$parent_geom` are visible.
  `$children` holds the child count.
- `for` loops over ranges, vectors, strings, scalars, and `undef`, with
  multiple bindings, `$` loop variables, and block or single-statement bodies.
  Bindings are evaluated sequentially, so later bindings can use earlier loop
  variables. `undef` yields no iterations and scalars yield one, matching
  OpenSCAD.
- `if` / `else` with boolean or numeric truthiness.
- `$fn` switches between faceted and analytic geometry at 33 segments.
  `$fa=12`, `$fs=2`, `$fn=0`, `$t=0`, `$preview=false`, and `PI` are initialized
  to OpenSCAD defaults; `$fa`/`$fs` only affect 2D profile sampling.
- `cube`, `cylinder`, `sphere`, `polyhedron`; `cube` and `cylinder` accept a
  per-call `center`. `cylinder` accepts OpenSCAD's positional
  `(h, r1, r2, center)` order and `r`/`d`/`r1`/`r2`/`d1`/`d2` aliases, and
  builds polyhedral frustums and cones. `polyhedron` accepts `triangles` as an
  alias for `faces`.
- `square`, `circle`, `polygon` sketches and `linear_extrude` with optional
  `center`, `twist`, `scale`, and `slices`, plus ignored `convexity`. Multiple
  2D children are unioned; twist/scale require a single contour without holes.
- `rotate_extrude` with optional sweep and start angles and ignored
  `convexity`.
- `translate`, `rotate`, `scale`, `multmatrix`, `mirror`, `resize`;
  `translate` accepts 2D vectors and pads Z with zero. `rotate` accepts a
  number, an Euler vector, or `a=`/`v=` axis-angle. `multmatrix` accepts
  3x3, 3x4, 4x3, and 4x4 row-major matrices.
- `hull` and `minkowski` for convex 3D children. Minkowski sums take their
  vertices from the B-rep rather than from tessellation, and `minkowski` with a
  single centred `sphere(r)` is exact: it becomes a rounded offset of the other
  child. 2D and non-convex Minkowski are not supported.
- Nodes carry a 2D/3D dimension; transforms preserve it, booleans require 3D,
  and `linear_extrude` unions multiple 2D children into a solid.
- `difference`, `union`, `intersection` on 3D solids and on 2D profiles.
  Each direct boolean child is grouped: a module call that emits several shapes
  is implicitly unioned before it is used as an operand.
- User module call arguments are merged into the returned part parameters, so
  values passed at the call site remain visible in the properties panel.
- 2D booleans run on polygonal profiles (clipper2-rust); results may contain
  holes or multiple disjoint regions.
- `offset(r=...)`, `offset(delta=...)`, and `offset(r=..., chamfer=true)` for 2D
  profiles. `r` uses round joins, `chamfer` switches `r` to bevel joins, and
  `delta` uses miter joins.
- Positional and named arguments (`size=`, `r=`/`h=`, `v=`, `a=`, `m=`,
  `newsize=`); unknown names are rejected.
- Arithmetic on numbers and lists: `+`, `-`, `*`, `/`, `%`, `^`, unary signs,
  and grouping. `+`/`-` are elementwise and recursive; `*` follows OpenSCAD's
  vector dot product and vector/matrix multiplication rules; scalar
  multiplication and division scale lists recursively. Other mixed list/scalar
  operations are `undef`, matching OpenSCAD. Division or modulo by zero follows
  IEEE `inf`/`nan` behavior. `undef` propagates through arithmetic.
- Nested transforms compose into one matrix before reaching a primitive.
- Each evaluated top-level shape exposes:
  - its final mesh
  - its source range
  - numeric parameter bindings
  - selectable operand parts (leaf primitives) with their own meshes,
    source ranges, and parameter bindings
  - a source-shaped tree of groups and leaves; leaves index into `parts`
- Transform parameters are inherited by the leaf operands, so they appear in
  the properties panel and remain editable.
- Leaf meshes are cached across evaluations. The key covers the module, its
  resolved numeric parameters, the composed world transform, and the
  tessellation tolerance, so only leaves whose geometry changed are
  re-tessellated.

### Kernel

- `kernel-api` is a capability-based `Kernel` trait with opaque `ShapeId`s.
- `kernel-truck` supports:
  - cuboid, cylinder, sphere, and polyhedron primitives
  - square, circle, and polygon sketches
  - linear extrusion of a sketch along Z
  - rotational extrusion of a sketch around Z
  - matrix transforms on faces and solids
  - union, difference, intersection on solids
  - tessellation to a neutral triangle mesh with the exact surface normals
    from the B-rep, so curved faces shade smoothly without mesh-derived
    smoothing
  - axis-aligned bounds for solids and sketches
  - analytic ray casting: planes, surfaces of revolution of a straight line
    (cylinders, cones), tensor-product B-spline/NURBS patches and balls are all
    exported as trimmed analytic faces. A revolution of a smooth curve
    (spheres, tori, revolved profiles, hyperboloids) is converted to an exact
    NURBS patch; the GPU tracer evaluates splines directly, so tessellation is
    only a fallback for a patch whose degree exceeds its basis window.
- Truck is the only implemented backend. OCCT and SolidCore are candidates,
  not adapters yet.
- `truck-shapeops` is vendored with a local fallible-face patch: malformed
  generated wires return `None` and trigger the boolean retry path instead of
  panicking. See [`vendor/README.md`](../vendor/README.md).

### Viewer

- eframe/egui UI with an offscreen wgpu render target registered as an egui
  texture.
- Source editor + Evaluate button (Ctrl+Enter).
- Evaluation runs on a worker thread; the UI stays responsive with a spinner,
  queues the latest request, and discards stale results.
- Orbit camera: drag to orbit, scroll to zoom.
- CPU ray picking against the analytic part scenes, with operand meshes as the
  fallback.
- Analytic compute-shader preview (the top-bar checkbox) over the same packed
  scene, including planes, cylinders, cones, balls and B-spline/NURBS patches.
- Shapes panel showing the source-shaped tree of groups and leaf primitives.
  Clicking a leaf selects it, including cutters enclosed by other geometry.
- Properties panel with drag-editable numeric parameters.
- "Legacy OpenSCAD centering" checkbox in the top bar.
- Parameter edits apply on drag release, Enter, or focus loss.
- Edits write back to the source, then re-evaluate.
- Built-in sample: `assets/sample.scad`, shared by the CLI, viewer, and
  `hello_cube` example.
- Native Open, Save, and Save As dialogs through `rfd`; Ctrl+S saves and
  Ctrl+Shift+S saves as. The current path appears in the window title, and
  modified source is marked with `*`.
- Evaluation uses the editor text with the open file's path, so unsaved edits
  resolve relative includes and `use` statements.

- `scripts/screenshot-viewer.sh` captures the UI headlessly through Xvfb and
  ffmpeg, so verification does not open a window on the desktop.

## Important decisions

- Source-faithful editing: `.scad` text is canonical; inspection state is not
  written back.
- Module and function definitions are collected at the start of each source
  file or block, then executed in source order. This permits forward references
  within a block while keeping nested definitions scoped to their enclosing
  module.
- Module, function, and loop bindings use persistent environments. Normal
  variables and declarations resolve lexically through the environment where
  the module or function was defined; `$` variables resolve dynamically through
  the caller chain.
  Call-site children of a module are evaluated in the caller's environment, so
  they see caller variables; `$` arguments passed to the module are re-applied
  while children are rendered.
- A user module returns its body shapes directly, without an implicit union.
  This matches the top-level behavior and avoids extra booleans; transforms and
  booleans above the call union them when one result is required.
- `include` is textual: the included file's assignments, definitions, and
  top-level geometry are evaluated in the including scope at the include
  location. `use` imports declarations from a hidden file scope, matching
  OpenSCAD. Both resolve relative to the current source file and are tracked by
  canonical path for cycle detection.
- Included-file source ranges stay relative to their own files. The editor does
  not yet map those ranges back to a file tab or highlight them.
- B-rep first. The kernel-neutral `Transform` is a column-major 4×4 affine
  matrix; `compose` applies the child first.
- OpenSCAD rotation order for `rotate([x, y, z])`: X, then Y, then Z.
- `multmatrix` rows are converted from OpenSCAD's row-major form into the
  internal column-major matrix.
- Singular or non-finite transforms are rejected before reaching the kernel.
  Reflections keep a negative determinant; the Truck adapter flips face
  orientation so mirrored solids stay outward-facing.
- `resize` fits the kernel bounds of its children in local space, then applies
  the accumulated parent transform. A zero dimension is left unchanged; the
  `auto` parameter is not supported.
- Truck is first because it is pure Rust, Apache-2.0, and needs no C++
  toolchain. OCCT is the likely C++ fallback; SolidCore is a watch item.
- `$fn` below 33 builds a faceted B-rep with exactly that segment count for
  cylinders and 2D profiles; `$fn` at 33 or above, or unset, uses analytic
  geometry where available. Spheres ignore `$fn` entirely — see below.
  `$fa`/`$fs` do not choose the geometry representation; they only sample
  2D profiles for CSG.
- `sphere` ignores `$fn` and is built as an exact surface of revolution: a
  meridian arc swept about the z axis, closed by two tiny polar caps. Nothing in
  the libraries checked uses a low `$fn` on a sphere to build a polyhedron —
  BOSL2's code never passes a literal below 16 and asks for facets through
  `spheroid(style = "icosa")` instead, and the only low value in the corpus is
  MCAD's Braille dot (`$fn = 8`, a render-cost shortcut).
- The exact sphere used to cost booleans. `circle_arc` stores the meridian as a
  rational quadratic NURBS, so the surface is exact and exports as a surface of
  revolution, but shapeops meshes each face and Newton-solves every intersection
  against that revolved curve. `Surface`'s nearest-parameter search for a
  revolved surface now delegates to the surface's own analytic search instead of
  using a 101×101 presearch grid as a Newton seed: `cube − sphere(1.2)` went
  from 9.0 s to 36 ms in the release CLI. Exactness is deliberate — an exact
  B-rep is what a STEP export needs, and a faceted one would ship 960 planar
  faces. The preview and picking do not pay for it: the kernel knows a shape is
  a ball and `raycast_scene` emits one exact `Surface::Sphere`, traced by a
  quadratic.
- A revolved surface's nearest-parameter search has no continuity hint when
  shapeops evaluates an intersection curve. `proj_angle`'s choice of seam side
  is rounding noise there, so `seam_side` keeps the seam at 0 rather than 2π.
  The old 101×101 grid did the same by tie-breaking to its first sample. Without
  the rule the sphere patches were trimmed to half their area even though every
  search returned the correct 3D point.
- `cylinder` revolves a rectangular profile about the axis instead of extruding
  a disk. The vertical edge becomes four analytic `Surface::RevolutedCurve`
  patches, the radial edges are rebuilt as planar caps from the side's own
  boundary arcs (a swept radial line collapses at the axis, where normals are
  undefined), and the on-axis edge is dropped. Extruding a disk instead leaves a
  homotopy B-spline wall: slower to subdivide, harder for booleans, and
  invisible to the ray caster. The cap winding is chosen by trying the
  combinations until the shell validates, then flipped if the solid faces
  inwards.
- `square` and `circle` profiles are centered in XY, `polygon` points are used
  as given, and extrusions are centered on Z, matching the centered 3D
  primitives rather than OpenSCAD's origin-at-a-corner convention.
  `EvaluationOptions::legacy_centering` switches cubes and squares to the
  origin and cylinders/extrusions to base `z = 0`; the CLI flag and viewer
  checkbox expose it.
  Profile children are filtered out of the selectable-part tree; the extrusion
  itself is the selectable part.
- `rotate_extrude` maps the profile's local X to radius and local Y to Z, then
  revolves around Z. `start` rotates the profile about Z before sweeping.
  Positive sweeps are flipped to keep outward orientation; profiles are
  required to lie at `x >= 0`.
- `polyhedron` welds coincident vertices and reorients inverted shells by signed
  volume before building shared edges. Faces must still be planar, closed, and
  consistently wound after welding.
- 2D CSG is a profile operation above the kernel: outer contours and holes are
  clipped with `clipper2-rust`, then rebuilt into one or more kernel faces.
  Analytic sketch faces remain the fast path for non-boolean extrusions;
  circles are flattened to 64 segments only when profiles enter 2D CSG.
- Multi-region profile results extrude or revolve per region and combine the
  resulting shells into one solid. Empty 2D boolean results are rejected because
  the model has no representation for empty geometry.
- `offset` inflates the profile with Clipper2, then runs an even-odd union to
  rebuild hole ownership. `r` selects round joins, `delta` miter joins, and
  `chamfer=true` with `r` selects bevel joins; the offset distance is applied in
  local profile space before the parent transform.
- `linear_extrude` centers the extruded solid on Z by default. `center=false`
  starts it at `z = 0`; `center=true` forces centering even with the legacy
  option enabled.
- Ray casting is a separate, backend-neutral layer. `brepscad-raycast` models
  trimmed planes, revolved lines (cylinders, cones), tensor-product B-spline and
  NURBS patches, and balls, all through trim loops in surface parameter space.
  `TruckKernel::raycast_scene` classifies each face, exports B-spline and NURBS
  faces as spline patches, converts a revolved B-spline or NURBS generatrix to
  the exact NURBS surface it sweeps, and emits one exact `Surface::Sphere` for a
  shape it knows is a ball. Cylinders are analytic because `cylinder` revolves a
  profile line instead of extruding a disk, which is what Truck turns into a
  homotopy B-spline wall.
- Boolean results are tessellated at a display tolerance of at least `0.05`
  (`BOOLEAN_DISPLAY_TOLERANCE`), because master Truck can subdivide trimmed
  boolean surfaces pathologically at finer tolerances: a cylinder 0.01 taller
  than its cube takes minutes at `0.01` and milliseconds at `0.05`. The
  consequence is visible on hole rims, which are polygonal in the mesh preview;
  an analytic renderer is the fix for silhouettes, not a finer mesh.
- Truck boolean success is tolerance-sensitive, so failures retry at finer
  tolerances instead of raising a fixed clearance requirement. The retry ladder
  (nudges and translated operands) is bounded by a 500 ms wall-clock budget, so
  a degenerate model reports a failure instead of stalling the UI.
- Booleans can produce a solid with no shells (an intersection that misses, or a
  subtraction that removes the whole target). Empty solids are kept as values
  and folded away at the next boolean instead of being fed back into shapeops,
  which cannot index an empty shell.
- A union that involves a multi-shell solid keeps the shells separate instead of
  asking shapeops to fuse every shell pair; those attempts are the slowest path
  in the backend and rarely succeed on degenerate input.
- Backend adapters should remain separate crates behind the common trait.
- Native fast paths for common library constructs (for example, BOSL2 rounded
  boxes) are intended to avoid mesh-CSG workarounds; `scad-intrinsics` is not
  created yet.

## Commands

The project uses Nix/direnv because Rust is not installed system-wide.
`shell.nix` provides `rustup`; `rust-toolchain.toml` pins Rust 1.95.0.

```sh
cd /home/widget/code/misc/visual-openscad
direnv allow .
nix-shell --run 'cargo test --workspace'
nix-shell --run 'cargo run -p brepscad-viewer'
```

Full check:

```sh
nix-shell --run 'cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace'
```

CLI:

```sh
nix-shell --run 'cargo run -p brepscad -- path/to/model.scad'
nix-shell --run 'cargo run -p brepscad -- --legacy-centering path/to/model.scad'
```

The viewer needs a working wgpu adapter (Vulkan or GLES). On this machine it
runs under Wayland. A quick smoke test is:

```sh
nix-shell --run 'timeout 8s target/debug/brepscad-viewer'
```

Exit code `124` means it stayed alive until the timeout and did not panic.

For headless UI snapshots, see
[`development.md`](development.md#headless-ui-snapshot).

## Known limitations

- Parser/evaluator subset is small:
  - included-file source ranges are not yet mapped back to an editor file
  - ranges are represented as lists rather than a distinct value type
  - indexing selects a single element or row; range/list slices are not
    supported (string character indexing is)
  - `children([...])` accepts a range or list of indices, but nested selection
    and other child expressions are not supported
  - `lookup` requires numeric keys; `search` handles the common string,
    number, and list-of-targets forms but not the full OpenSCAD matching API
  - `rands` uses a fixed default seed when none is supplied, so it is
    deterministic and does not match OpenSCAD's time-seeded output
  - module bodies return their shapes directly, so a module call does not add a
    group node of its own to the source-shaped tree
  - `#`, `%`, and `!` parse and evaluate, but do not highlight, ghost, or isolate
    the modified subtree in the viewer
  - `for` / `if` results used where exactly one child is required (extrusions,
    offset) still error if they emit zero or multiple shapes
  - 2D booleans and offsets are polygonal; circles flatten to sampled profiles
    and there is no curve intersection support
  - `$fa`/`$fs` do not affect geometry; they only sample 2D profiles
  - empty 2D boolean or offset results are reported as errors
  - `linear_extrude` supports `height`, `center`, `twist`, `scale`, and
    `slices`, and ignores `convexity`. Twist/scale require a single contour
    without holes.
  - `rotate_extrude` requires `x >= 0`, ignores `convexity`, and has no
    negative-side profiles
  - `polyhedron` requires closed, consistently wound planar faces
  - `sphere` ignores `$fn` and is an exact surface of revolution, so it exports
    as a surface rather than facets; booleans against one are now cheap (about
    36 ms for `cube − sphere(1.2)` in release) but their display mesh is
    tessellated at the coarse boolean tolerance
  - `hull` and `minkowski` support convex 3D children only; 2D hulls and
    non-convex Minkowski sums are not implemented
  - `minkowski` with a sphere is exact only for a box; the rounded offset's
    patches are built as revolution sectors and do not share topology, so its
    display mesh has hairline cracks at the seams
  - evaluation stops at 256 nested scopes, so deeply recursive library code
    reports a call-depth error instead of overflowing the stack
- `color` and `render` are accepted as pass-throughs; color is not rendered.
- Arithmetic division and modulo by zero follow IEEE `inf`/`nan` behavior.
- `resize` supports a positional `newsize` only; `auto` and named arguments are
  not supported. It asks the kernel for bounds, which are tessellation-derived,
  so it is slower than a matrix-only transform.
- Truck booleans try exact tolerances `0.05`, `0.02`, then `0.01`, then a tiny
  difference-only corner nudge, then translated-operand retries. Disjoint
  unions combine their shells directly, and difference can subtract from or by
  a multi-shell compound shell by shell. Overlapping unions that Truck cannot
  fuse still fail. The vendored shapeops patch prevents malformed wires from
  panicking the application.
- Truck booleans can still fail on touching or coincident faces, and master
  shapeops has no spatial acceleration: an attempt costs one intersection-curve
  solve per face pair. Results are also not deterministic across runs, because
  Truck hashes topology IDs by `Arc` pointer address, so `FxHashMap` iteration
  order follows the process address layout.
- The opengrid bin generator is the working stress case. Its 1x1 bin and the
  2x2 bin body render reliably and in under a second. The full 2x2 model
  (`bin()` plus `snap_grid()`) renders in about a third of runs; the rest either
  report a boolean failure or stall in a pathological boolean or tessellation
  step. The model also feeds `prism_stack()` tapered prisms whose caps collapse
  to a knife edge, which produces self-intersecting polyhedron faces and
  coincident faces between solids.
- Picking uses the analytic part scenes when the analytic preview is on; mesh
  triangles remain the fallback for parts without a scene.
- Selection:
  - per leaf primitive, not per nested boolean group
  - no selected-part highlight
  - enclosed cutters are reachable from the Shapes tree, but there is no
    cycling between overlapping parts
- Native file dialogs need a working desktop portal (or Zenity fallback) on
  Linux; headless Xvfb runs cannot open them.
- No export, undo/redo, or session persistence. Source text is in memory until
  saved.
- Included-file source ranges are not mapped back to editor tabs.
- Truck transitive dependencies `nom 3.2.1` and `quick-xml 0.22.0` emit
  future-incompatibility warnings.
- Licence is BSD-3-Clause with a placeholder copyright holder; replace before
  distributing.

## Recommended next steps

A corpus run over BOSL2 and MCAD (109 files) now parses every file. Full
evaluation succeeds on 89 files (88 with no geometry, plus BOSL2
`examples/lsystems.scad`), with 9 kernel boolean failures, 1 BOSL2 assertion
failure, 3 language errors from standalone library files, 2 geometry errors,
4 other evaluation errors, and 1 timeout (`spring_handle.scad`, which is
functionally correct at reduced size but too slow at its default size). The arithmetic, built-in, primitive,
color/render, loop, search, and empty-boolean gaps from the previous pass are
fixed; the remaining failures are mostly library-context, kernel, and deeper
BOSL2 math/semantics issues.

“ok” here only means the file evaluated without an error; nearly all are pure
library files with no top-level geometry. BOSL2’s module surface is now partly
usable: `std.scad` loads, and `cube`, `cuboid`, `cylinder`, `sphere`,
`prismoid`, `rect_tube`, and `torus` render through it. `cuboid(chamfer=)` and
`cuboid(rounding=)` also render; non-convex Minkowski sums remain limited.

1. **Finish the ray-casting layer**
   - The analytic preview covers planes, cylinders, cones, balls and
     B-spline/NURBS patches, including surfaces of revolution converted to
     NURBS; picking already prefers the analytic part scenes.
   - Patches whose degree exceeds the shader's basis window still keep
     tessellated coverage. Raise the window or add a fallback tracer.
   - Prototype screen-space adaptive sampling for the compute preview.
2. **Remaining library and kernel gaps**
   - Make Truck booleans deterministic: topology IDs hash `Arc` pointers, so
     results depend on the process address layout. A stable per-entity ID would
     make runs reproducible and make failures debuggable.
   - Give shapeops a cost bound it can actually honour. Bounding the retry
     ladder is not enough; a single attempt over a large face-pair set, and
     tessellation of a degenerate boolean face, can still run for minutes.
   - Improve Truck boolean robustness for the MCAD bitmap/hardware models.
   - Render `color` instead of ignoring it.
   - Extend `hull`/`minkowski` to 2D and non-convex uses.
   - Treat standalone BOSL2 files that expect `std.scad` as an expected corpus
     category rather than a language gap.
3. **Keep the pinned corpus green**
   - Run `scripts/fetch-scad-corpus.sh` and then
     `scripts/check-scad-corpus.sh .corpus` after language changes.
   - Use parse-only runs for syntax coverage and `--evaluate` to separate
     language errors from geometry and kernel gaps.
   - Refresh `corpus/baseline-parse.tsv` and `corpus/baseline-evaluate.tsv` only
     after intentional compatibility improvements.
4. **Finish the 2D/extrusion layer**
   - Polygonal profile smoothing and `$fn` sampling controls for offsets.
   - `linear_extrude` negative-side profiles.
5. **Native intrinsics**
   - Start with the common BOSL2 `cuboid(rounding=)` case.
   - Always fall back to ordinary evaluation.
   - Version intrinsics by source path/content hash.
6. **Kernel adapters**
   - Add OCCT and SolidCore behind feature flags/crates.
   - Build the conformance harness before claiming capabilities.
7. **Product surface**
   - Recent-file handling.
   - Export, undo/redo, and session persistence.
8. **Selection polish**
   - Highlight selected operand.
   - Ghost or cycle overlapping parts.

## Relevant docs

- `docs/status.md` — this file: current capabilities, limitations, and next
  steps.
- `docs/notes/2026-10-30.md` — first-session snapshot and checkout sync note.
- `docs/notes/2026-10-31.md` — syntax-compatibility slice and corpus results.
- `docs/notes/2026-11-01.md` — Truck master migration, exact spheres, rounded
  offset, and the opengrid investigation.
- `docs/notes/2026-11-02.md` — analytic parameter search and the seam rule that
  makes sphere booleans cheap.
- `docs/research.md` — prior art, source-first architecture, kernel trade-offs,
  native library fast paths.
- `docs/kernels.md` — per-operation OCCT/SolidCore/Truck capability matrix.
- `README.md` — current build/run instructions and supported subset.
