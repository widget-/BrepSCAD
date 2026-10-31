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
scripts/
  screenshot-viewer.sh
vendor/
  truck-shapeops/ patched Truck boolean crate
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
  Functions can recurse, call other functions, and read caller variables.
  Function literals (`function(x) ...`) are first-class values: they capture
  their defining scope, can be passed, returned, stored in lists, and invoked
  with `f(...)` or chained calls such as `f(2)(3)`. `is_function` recognizes
  them. Conditional expressions (`? :`) evaluate only the selected branch.
- Built-in functions: `abs`, `sign`, `sqrt`, `ln`, `log`, `exp`, `floor`,
  `ceil`, `round`, the degree-based trig functions, `atan2`, `pow`, `min`,
  `max`, `norm`, `cross`, `len`, `concat`, `str`, `chr`, `ord`, `lookup`,
  `search`, `rands`, `is_num`, `is_bool`, `is_list`, `is_string`, `is_undef`,
  `is_function`, `version`, and `version_num`.
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
  definitions. `use` imports only module and function definitions. Quoted paths
  resolve relative to the including file; angle-bracket paths also search
  `OPENSCADPATH` and `--library-path` directories. Cycles and read/parse
  failures are reported with the file path.
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
- Module and loop scopes shadow caller variables and restore them on exit.
  Module and function bodies currently read caller variables (dynamic scoping).
  OpenSCAD resolves normal variables lexically at the definition site, so this
  is a known divergence; `$` variables are dynamic in both.
- `children()` renders all call-site children; `children(n)` selects one index
  and `children([...])` selects a range or list of indices. Children are
  evaluated in the caller's scope at the transform where `children()` appears.
  `$children` holds the child count.
- `for` loops over ranges and vectors, with multiple bindings and block or
  single-statement bodies.
- `if` / `else` with boolean or numeric truthiness.
- `$fn` switches between faceted and analytic geometry at 33 segments;
  `$fa`/`$fs` only affect 2D profile sampling.
- `cube`, `cylinder`, `sphere`, `polyhedron`.
- `square`, `circle`, `polygon` sketches and `linear_extrude` with optional
  `center`.
- `rotate_extrude` with optional sweep and start angles.
- `translate`, `rotate`, `scale`, `multmatrix`, `mirror`, `resize`.
- Nodes carry a 2D/3D dimension; transforms preserve it, booleans require 3D,
  and `linear_extrude` turns exactly one 2D child into a solid.
- `difference`, `union`, `intersection` on 3D solids and on 2D profiles.
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
  and grouping. List operations are elementwise and recursive; mixed
  list/scalar arithmetic is `undef`, matching OpenSCAD. Division or modulo by
  zero is an expression error. `undef` propagates through arithmetic.
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
  - tessellation to a neutral triangle mesh
  - axis-aligned bounds for solids and sketches
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
- CPU ray picking against operand meshes.
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
- Module, function, and loop variables use a scope stack. Assignments write to
  the innermost scope, and lookups fall back outward (dynamic scoping). This is
  a known divergence from OpenSCAD for normal variables, which are lexical at
  the definition site; only `$` variables are dynamic. Resolving the scope
  model is planned with assignment hoisting.
  Call-site children of a module are evaluated after temporarily removing the
  module's scope, so they see the caller's variables; `$` arguments passed to
  the module are re-applied while children are rendered.
- A user module returns its body shapes directly, without an implicit union.
  This matches the top-level behavior and avoids extra booleans; transforms and
  booleans above the call union them when one result is required.
- `include` is textual: the included file's assignments, definitions, and
  top-level geometry are evaluated in the including scope at the include
  location. `use` imports declarations only, matching OpenSCAD. Both resolve
  relative to the current source file and are tracked by canonical path for
  cycle detection.
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
- `$fn` below 33 builds a faceted B-rep with exactly that segment count;
  `$fn` at 33 or above, or unset, uses analytic geometry where available.
  `$fa`/`$fs` do not choose the geometry representation; they only sample
  2D profiles for CSG.
- `sphere` is built as a faceted UV B-rep with shared edges and planar
  triangles. Revolving a half-disc in Truck produced degenerate pole faces that
  broke tessellation and booleans, so the faceted shell is used instead.
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
- `polyhedron` builds shared edges from the input points and faces, then relies
  on `Solid::try_new` to validate the shell. Faces must be planar and the input
  must be closed and consistently wound; no repair is attempted.
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
- Truck boolean success is tolerance-sensitive, so failures retry at finer
  tolerances instead of raising a fixed clearance requirement.
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
  - function literals capture variables at creation; later sequential
    reassignment is not reflected until assignment hoisting is implemented
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
  - `linear_extrude` supports `height` and `center`; no twist, scale, or slices
  - `center` is not supported per primitive; OpenSCAD placement is a global
    evaluation option
  - `rotate_extrude` requires `x >= 0` and has no negative-side profiles
  - `polyhedron` requires closed, consistently wound planar faces
  - `sphere` is faceted; `$fn` selects coarse counts, default 32×16
- No color handling.
- `resize` supports a positional `newsize` only; `auto` and named arguments are
  not supported. It asks the kernel for bounds, which are tessellation-derived,
  so it is slower than a matrix-only transform.
- Truck booleans try exact tolerances `0.05`, `0.02`, then `0.01`, then a tiny
  difference-only corner nudge, then translated-operand retries. Some
  configurations still fail. The vendored shapeops patch prevents malformed
  wires from panicking the application.
- Truck booleans can still fail on touching or coincident faces.
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
evaluation succeeds on 85 files (all pure definitions), with 3 language errors,
2 assertion failures, 2 unsupported geometry modules, and 17 other evaluation
errors. Assignment and declaration hoisting now let included BOSL2 files
progress to arithmetic and library-behavior gaps such as division by zero,
missing hyperbolic functions, and per-primitive `center`.

1. **Lexical scope semantics**
   - Resolve normal variables lexically at the definition site; keep `$`
     variables dynamically scoped.
   - Preserve the current assignment/declaration hoisting rules.
   - Unknown variables and missing arguments already evaluate to `undef`.
2. **Remaining corpus evaluation gaps**
   - Add missing built-ins such as `acosh` and the other hyperbolic functions.
   - Support per-primitive `center` on `cube` and `cylinder`.
   - Decide how to represent empty boolean/union results.
   - Resolve the remaining BOSL2/MCAD module argument mismatches.
3. **Keep the pinned corpus green**
   - Run `scripts/fetch-scad-corpus.sh` and then
     `scripts/check-scad-corpus.sh .corpus` after language changes.
   - Use parse-only runs for syntax coverage and `--evaluate` to separate
     language errors from geometry and kernel gaps.
   - Refresh `corpus/baseline-parse.tsv` and `corpus/baseline-evaluate.tsv` only
     after intentional compatibility improvements.
4. **Finish the 2D/extrusion layer**
   - Polygonal profile smoothing and `$fn` sampling controls for offsets.
   - `linear_extrude` twist, scale, and slices.
   - `rotate_extrude` negative-side profiles.
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
- `docs/research.md` — prior art, source-first architecture, kernel trade-offs,
  native library fast paths.
- `docs/kernels.md` — per-operation OCCT/SolidCore/Truck capability matrix.
- `README.md` — current build/run instructions and supported subset.
