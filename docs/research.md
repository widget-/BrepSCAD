# Visual OpenSCAD --- prior art and architecture research

Last updated: 2026-10-28
Status: research notes. Current direction as of 2026-10-28: **B-rep-first**. Exact OpenSCAD output identity is out of scope (users who need it should stay on OpenSCAD). Candidate backends are **OCCT, SolidCore, and Truck**; Manifold is not a selectable backend and may only survive as an internal mesh fallback for `hull`/`minkowski`/mesh imports. The per-operation capability matrix lives in [`docs/kernels.md`](kernels.md).

## 1. The question

Can we build a Fusion/Onshape-like visual editor on top of plain `.scad` files, where:

- `.scad` text remains the canonical document;
- the tree reads as close to the source as possible;
- edits write back into the source immediately;
- inspection state (isolated part, hidden body, preview at a step) is session-only and never serialized;
- shape parameters can be edited through a properties panel and/or anchored gizmos;
- constraint-style relationships are expressed through variable assignments in the source?

And what already exists that does this?

## 2. Short answer

No open-source project appears to match the whole combination. The closest existing work splits into four groups:

1. **Hosted/proprietary visual OpenSCAD** --- OwlCAD is the closest UX match (`.scad` → editable model tree + editable history), but it is a hosted product, not open source, and its marketing copy is heavy enough that maturity should be treated as unverified.
2. **Open-source tree ↔ `.scad` editors** --- KherveCAD and 3dscad. KherveCAD uses its own `.kcad` document and treats `.scad` as an import/export format; 3dscad is a Qt prototype that round-trips only a defined subset.
3. **Source-faithful OpenSCAD workbenches** --- BelfrySCAD is the strongest reference. It keeps plain `.scad` as the document, implements the full language, maps AST nodes to geometry IDs for picking, and has a debugger that shows the partial model at every pause.
4. **Timeline-scrubbing code CAD** --- Cascade Studio has a "Modeling History Timeline" that scrubs build steps, and an OpenSCAD mode, but it transpiles SCAD into its JavaScript/OCCT library rather than evaluating the SCAD tree directly.

**Kernel correction:** Manifold is the fast new *mesh CSG* kernel; OpenCASCADE is a *B-rep* kernel. They are different classes of tool, and one does not replace the other. See §6 and [`kernels.md`](kernels.md).

**Current bottom line:** build v1 B-rep-first --- Rust + lossless CST + a separate instantiated model tree + wgpu + egui. Exact OpenSCAD output identity is out of scope; users who need it should use OpenSCAD itself. Candidate backends are OCCT, SolidCore, and Truck, with a capability matrix in [`kernels.md`](kernels.md). Manifold is no longer a selectable backend; keep a small internal mesh path for `hull`, `minkowski`, and imported meshes. Treat "timeline" as subtree/step inspection, not a Fusion-style linear feature history, unless the source language is deliberately restricted to a linear feature subset.

## 3. Prior art

### 3.1 Comparison

| Project                        | Form                                            | Source of truth                                                                 | Tree / history                                                                        | Kernel                                                                        | Status / licence                                                                             |
| ------------------------------ | ----------------------------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| **OwlCAD**                     | Hosted browser app                              | Its own document; can import `.scad` into an editable tree or run a live script | Editable parametric history timeline; scene tree                                      | Not stated; browser/WebWorker                                                 | Proprietary, hosted; free to use with Pro features. Claims are from its own marketing pages. |
| **KherveCAD**                  | Python/PyQt5 desktop app                        | `.kcad` object tree; `.scad` import/export is claimed lossless                  | Object tree, live Code tab, errors mapped back to objects                             | Bundled OpenSCAD engine                                                       | Source archived on Zenodo; author describes it as free/open-source.                          |
| **3dscad**                     | Qt5 prototype                                   | A supported `.scad` subset, round-tripped                                       | Graphics tree + classic tree; source mapping; viewport picking                        | Manifold if built, otherwise approximate mesh/box CSG; can bridge to OpenSCAD | Prototype; subset only.                                                                      |
| **BelfrySCAD**                 | Desktop app (Python/PySide6 UI + C++ evaluator) | Plain `.scad`                                                                   | No parts timeline; debugger with live partial model; CSG tree dump; AST↔geometry IDs  | Manifold only                                                                 | MIT; active as of Oct 2026.                                                                  |
| **Cascade Studio**             | Browser IDE                                     | JavaScript (or transpiled SCAD)                                                 | Modeling history timeline to scrub build steps                                        | OpenCASCADE (OCCT 8 RC)                                                       | MIT; 1.5k stars.                                                                             |
| **OpenSCAD Graph Editor**      | Desktop node editor                             | A node graph that generates `.scad`                                             | Node graph, not a source tree                                                         | Delegates to OpenSCAD for preview                                             | GPL-3.0; last release Nov 2023; does not import arbitrary `.scad`.                           |
| **GraphSCAD**                  | Desktop node editor                             | Python script that generates `.scad`                                            | Node graph                                                                            | Delegates to OpenSCAD                                                         | Open source; low activity.                                                                   |
| **FreeCAD OpenSCAD workbench** | FreeCAD workbench                               | FreeCAD document                                                                | FreeCAD tree                                                                          | OCCT                                                                          | Imports `.scad` only via `.csg`; expands loops and loses parametrization.                    |
| **ClawSCAD**                   | Desktop AI wrapper                              | `.scad` checkpoint files                                                        | Checkpoint history/branching, not construction steps                                  | OpenSCAD                                                                      | Recent; adjacent, not the same idea.                                                         |
| **SolidCraft**                 | Pure Rust desktop/web/CLI                       | Its own `.sc` design with a feature timeline                                    | Editable parametric timeline; own constraint solver                                   | `solidcore` (Rust port of NVIDIA SMLib)                                       | MIT/Apache-2.0; early but usable; strong Rust architecture reference.                        |
| **Open CAD Studio**            | Pure Rust desktop                               | DWG/DXF/STEP etc.                                                               | CAD document                                                                          | Own geometry                                                                  | GPL-3.0; iced + wgpu; 2D/3D CAD reference.                                                   |
| **BREP**                       | Rust crate family                               | Rust B-rep model                                                                | eframe/egui app + wgpu renderer                                                       | Own NURBS B-rep kernel                                                        | Early; useful as a stack reference.                                                          |
| **ConstructCAD**               | Rust BIM app                                    | `.ccad` RON                                                                     | Document tree                                                                         | Own CSG/B-rep mix                                                             | egui + wgpu; BIM rather than SCAD, but a useful Rust CAD architecture.                       |

### 3.2 The closest five, in more detail

#### OwlCAD

Public claims (from `owlcad.com`, checked 2026-10-28):

- Opens `.scad` and `.csg`; interprets modules, functions, variables, `for`/`if`, list comprehensions, `include`/`use`.
- "Lands the result as an editable model tree of primitives, booleans, and transforms."
- "An editable parametric history timeline you can scrub and retro-edit."
- Continuous evaluation in a Web Worker; no preview/render split.
- `polyhedron` and non-sphere `minkowski()` arrive as mesh; everything else is parametric.
- Hosted, proprietary, not open source.

Relevance: this is the closest description of the intended UX. It is a UX reference, not a code or dependency reference. The front page reads as heavily AI-assisted marketing, so treat feature claims and longevity as unverified.

#### KherveCAD

- Python + PyQt5; OpenSCAD engine bundled.
- Document is "a tree of objects that compiles one-to-one to an OpenSCAD program."
- Code tab is bidirectional: edit the text and apply it back into the object tree.
- Object selection highlights source lines; compiler errors map back to the object that caused them.
- Imports/exports `.scad` with a claimed lossless round-trip, including user modules and functions.
- Native document is `.kcad`, not `.scad`; the object tree is canonical.

Relevance: the closest open-source example of "object tree ↔ generated SCAD code" with bidirectional editing. Its choice to make the tree canonical is the opposite of the source-first model proposed here, but the code-mapping and error-mapping techniques are directly useful.

#### 3dscad

- Qt5/C++ prototype; "Tinkercad-like interface where UI actions update OpenSCAD code, and supported OpenSCAD code can be applied back into the visual scene."
- Visual tree, classic tree, source mapping that highlights the selected tree node in the editor.
- Viewport picking, transform gizmo, boolean modes (add/subtract/intersect).
- Async CSG preview; Manifold if available, otherwise approximate mesh/box CSG.
- Round-trips only a defined subset: top-level variables, module declarations/calls, boolean groups, transform groups, `for`, `linear_extrude`, `resize`, `color`, `hull`, `minkowski`, `polyhedron`, and core primitives.
- Known limitations: small scope model, no rename flow, graphics tree is still experimental.

Relevance: the closest prototype of the exact "visual tree over a parsed `.scad`, edit writes back" idea. Its subset contract is a useful warning: full-language round-tripping is a large project.

#### BelfrySCAD

- Reads/writes plain `.scad`, implements the full OpenSCAD language, uses Manifold only.
- No live preview while typing; F5 preview and F6 render both do a full CSG evaluation.
- Debugger pauses mid-evaluation and shows the call stack, variables, and the partial model built so far.
- AST is the source of truth; every geometry-producing node carries both its source span and the geometry IDs it produced, enabling ⌘-click selection and gizmo drag-to-edit.
- "Dump CSG Tree to Console" prints the resolved CSG tree.
- WYSIWYG drag-to-edit preserves parametric relationships: dragging a transform updates a delta rather than wrapping the expression in a new transform.
- No parts/timeline panel; the debugger is the "preview at a point" mechanism.

Relevance: the best reference for source-faithful evaluation, AST↔geometry provenance, and expression-preserving direct manipulation. Its Python/PySide6 + C++ evaluator implementation is not a stack recommendation, but its architecture is.

#### Cascade Studio

- Browser IDE; JavaScript or OpenSCAD mode; OCCT 8 RC via opencascade.js.
- "Modeling History Timeline to scrub through build steps in the 3D viewport."
- OpenSCAD mode transpiles SCAD into its JavaScript standard library; it does not evaluate the SCAD AST directly.
- MIT; Playwright tests; `cascade-core` can be embedded.

Relevance: the best existing demo of the scrub-timeline interaction. Its transpiler means the timeline is over generated JS calls, not over the original SCAD tree.

### 3.3 Adjacent work worth reading

- **Research prototype:** J. Felipe Gonzalez et al., *Introducing Bidirectional Programming in Constructive Solid Geometry-Based CAD* (arXiv:2408.01801, 2024). A modified OpenSCAD where clicking a 3D element reveals the CSG-tree nodes that produced it, difference cutters become selectable ghosts, and code↔model navigation works both ways. This is the best interaction-design reference for the proposed idea.
- **Node editors:** OpenSCAD Graph Editor and GraphSCAD generate `.scad` from a graph. They do not ingest arbitrary `.scad`, because the graph is the source of truth.
- **FreeCAD's OpenSCAD workbench:** imports `.scad` by converting it to `.csg`, which expands loops, evaluates variables, and discards parametrization. It is the canonical example of the lossy path to avoid.
- **Rust CAD apps:** SolidCraft (feature timeline + own constraint solver, egui + wgpu), Open CAD Studio (iced + wgpu), BREP (egui + wgpu), ConstructCAD (egui + wgpu). These are evidence that the Rust UI/GPU stack is viable for CAD-scale applications.

## 4. The source-first model

### 4.1 Use a lossless CST, not a plain AST

If the tree is supposed to read like the source and edits are supposed to go back into the file, the parser must preserve everything: whitespace, comments, parentheses, trailing commas, formatting quirks.

Use a lossless concrete syntax tree (CST):

- `rowan` (used by rust-analyzer) or `cstree` (thread-safe fork with persistent red trees).
- Green tree = immutable, position-independent syntax; red tree = typed view with absolute ranges.
- Every token and node carries a `TextRange`.
- Trivia (whitespace, comments, newlines) is kept in the tree, not discarded.
- A typed AST layer sits on top for semantic queries.

This gives:

- `tree.text() == source` byte-for-byte;
- precise source ranges for every node;
- error-tolerant parsing for live editing;
- the ability to apply a minimal `TextEdit` to the original text instead of pretty-printing the whole file.

Do not regenerate the file from an AST. Pretty-printing the whole document on every edit destroys comments, formatting, and diffs. Apply a range replacement and reparse only the affected subtree.

### 4.2 Two trees, synchronized

A single tree cannot be both "reads like the source" and "shows parts and evaluated shapes." A loop or module call is one source node but many geometry instances; a `difference()` is one source node but several operands plus a result.

Keep two layers:

```text
.scad text
  └─ lossless CST  ──►  typed AST / HIR
                          │
                          ▼
                    evaluator (scopes, modules, loops, children())
                          │
                          ▼
                 instantiated model tree / DAG
                 (each node: source span + stable path + shape handle)
```

- **Source tree (CST):** what the user edits. Mirrors text structure.
- **Model tree (instantiated):** what the user inspects. Mirrors evaluation. Nodes carry:
  - a stable ID: `(module call path, source span, loop iteration indices)`;
  - the source range of the construct that produced them;
  - the evaluated shape (or a cache handle);
  - the parameters actually used.
- Selecting a source node highlights all model instances it produced; selecting a model instance reveals its source.

This is the honest way to reconcile "tree reads like source" with "top-level unions are parts." The source tree is the editing surface; the model tree is the inspection surface. They are linked by source spans and stable IDs.

### 4.3 "Preview at a state" needs a precise definition

There are three different things that could be meant:

1. **Subtree preview** --- show the shape produced by one source construct (e.g. this `difference()`, this module call). This is well-defined: evaluate that subtree in its inherited parameter/transform context. Cutters can be shown as ghost volumes.
2. **Evaluation-step preview** --- step through the evaluator’s walk and show the model built so far. This is what BelfrySCAD’s debugger does. It is deterministic but is a debugging view, not a Fusion timeline.
3. **Fusion-style timeline scrub** --- show the model as of feature N. This is not natural for SCAD: source order is not dependency order, booleans are declarative, and a "feature" is an arbitrary grouping. Trying to impose a linear feature order will fight the language.

Recommendation: implement (1) and (2), and call the result an "inspect" or "step" view. Do not promise a Fusion timeline unless the source is deliberately restricted to a linear feature subset.

For `difference()`, "the shape at that point" is the parent’s result, not the cutter’s raw shape. The evaluator must retain per-node *result* shapes as well as operands, and the UI must distinguish "show this operand" from "show the result of this node."

### 4.4 Inspection state

Keep inspection state outside the document:

```rust
struct ViewState {
    isolated: HashSet<NodeId>,
    hidden: HashSet<NodeId>,
    ghosted: HashSet<NodeId>,      // e.g. difference cutters
    preview_root: Option<NodeId>,  // subtree preview
    selected: HashSet<NodeId>,
    // camera, section plane, measurement, etc.
}
```

- Never serialized into `.scad`.
- Optionally persisted in a workspace/session file, separate from the source.
- Rendering composes the full evaluated model, then applies visibility filters.
- For isolation, render the selected node’s cached result shape, optionally with ancestors/context as ghosts.

### 4.5 Incremental parse and evaluation

- On a text edit, apply a `TextEdit` to a rope (`ropey`), reparse the smallest affected CST subtree.
- Lower the changed CST nodes to HIR; re-resolve names in the affected scope.
- Evaluate from the changed model nodes downward.
- Cache geometry per node keyed by a content hash of:
  - resolved parameters,
  - child hashes,
  - transform,
  - relevant `$` variables (`$fn`, `$fa`, `$fs`, `$preview`, `$t`),
  - kernel/version.
- Keep per-node result shapes and operand shapes; this is what makes preview/ghosting cheap.
- Run parsing/evaluation off the UI thread. If OCCT is used, consider process isolation eventually: a kernel crash must not take down the editor.

## 5. Editing model

### 5.1 Value bindings

Every editable value should be represented as a binding, not a bare number:

```rust
struct Binding {
    range: TextRange,      // source span of the expression or literal
    expr: String,          // current source expression, e.g. "plate_w / 3"
    value: Value,          // evaluated value
    kind: ValueKind,       // length, angle, count, bool, vector...
    owner: NodeId,
}
```

The properties panel edits `expr`, not `value`:

- If the user types a number, replace the literal’s range.
- If the user types an expression, replace the expression’s range.
- If the value is a reference to a named variable, offer "edit variable" (changes all uses) versus "override here" (replaces this expression).
- Show both the expression and the evaluated value, so the user can see what the model will actually use.

### 5.2 Properties panel vs gizmos

Both are useful; they should edit the same binding model.

#### Properties panel

- Lists the selected node’s parameters with name, expression, evaluated value, unit.
- Edits are text edits; undo is a normal text-edit undo.
- Good for precise values, expressions, and non-geometric parameters (e.g. `$fn`, booleans, colors).
- Naturally supports multi-selection and "edit the shared variable" affordances.

#### Gizmos

- Translate: update the nearest enclosing `translate()` if it is the immediate parent and its vector is editable; otherwise insert a `translate()`.
- Rotate: update the nearest `rotate()`; otherwise insert one.
- Scale/resize: prefer primitive parameters (`cube(size=...)`, `sphere(r=...)`, `cylinder(h=..., r=...)`) over adding a `scale()`. If the shape is not a primitive, a `scale()` is the only option.
- If the value is expression-backed, do not overwrite the expression with a number. Add a delta or edit the underlying variable, so the parametric relationship survives. BelfrySCAD explicitly does this and is worth studying.
- Coalesce a drag gesture into one undo transaction on mouse-up.
- Snapping and inference are much easier with a B-rep kernel (faces, edges, vertices). With a mesh-only kernel, you can snap to triangles/bounding boxes or implement feature inference yourself.

### 5.3 Constraint-style relationships

"Constraints driven through variable assignment in the source" can mean several levels. Build them in this order:

**Level 0 --- explicit formulas.** The source already supports `wall = 2; hole_d = plate_w / 3;`. The UI can expose a parameter dependency graph and let the user edit formulas. No solver.

**Level 1 --- constrained assignments.** UI actions such as "make equal," "set ratio," "derive from," "offset by," or "match diameter" write a formula referencing the other parameter:

```scad
hole_d = bolt_d + clearance;
plate_w = hole_spacing * 2 + 20;
```

This is still ordinary SCAD and keeps the source readable. It covers a large fraction of real parametric intent without a solver.

**Level 2 --- 2D sketch solver.** For coincident/tangent/distance/angle constraints on a sketch, use a real solver. The proven implementations are C++: FreeCAD’s `planegcs` and SolveSpace’s `libslvs`. A Rust solver is possible (Levenberg–Marquardt on the residual system, with rigid-cluster/construction handling), but it is a project in itself. SolidCraft has its own solver and is a useful reference.

**Level 3 --- 3D assembly mates.** Another order of magnitude. Do not put this in v1.

The hard part is **write-back**. A numerical solver produces numbers; writing those numbers into the source destroys the formula and the parametric intent. Options:

- **Symbolic elimination:** solve for one variable and write an expression. Only tractable for simple linear systems.
- **Constraint annotations:** keep constraints in comments or a sidecar, solve at load, and write resolved values into a dedicated parameter block. The source remains valid SCAD, but it is no longer the sole source of truth.
- **Bake values:** simplest, but the worst fit for a source-first tool.
- **Formula-only constraints:** restrict the UI to relationships expressible as one-line assignments. This is the recommended v1.

Be explicit in the UI about which parameters are "driving" (source expressions) and which are "driven" (solver output). Do not silently convert a formula into a number.

## 6. Kernel choice: Manifold vs OpenCASCADE

OCCT is **Open CASCADE Technology**, the open-source B-rep kernel maintained by Open Cascade SAS; "OpenCascade" is commonly used as shorthand for OCCT.

### 6.1 They are different kinds of kernel

| | **Manifold** | **OpenCASCADE (OCCT)** |
| --- | --- | --- |
| Representation | Triangle mesh / CSG | B-rep (shells, faces, edges, vertices; analytic + NURBS surfaces) |
| Strength | Fast, robust mesh booleans; 2D cross-sections; extrude/revolve; hull; Minkowski; decomposition; ray casting | Fillets, chamfers, shells, drafts, STEP/IGES, exact topology, face/edge picking, measurement |
| Weakness | No B-rep topology, no STEP, no fillet/shell features | Slower for mesh-style CSG; stricter about invalid/non-manifold input; no first-class `hull`/`minkowski` |
| OpenSCAD fit | Direct: OpenSCAD's current development builds use Manifold as a backend; the 2021.01 stable release still uses CGAL | Indirect: OpenSCAD is mesh CSG, OCCT is B-rep |
| Licence | Apache-2.0 (upstream); Rust bindings Apache-2.0 OR MIT | LGPL-2.1 with exception; dynamic linking matters |
| Rust access | `manifold-csg` / `manifold3d` (C API, f64, wasm-capable) | `opencascade-rs` / `occt-sys` / `occt-wasm` |

**Manifold is the fast new kernel** in the sense that matters for OpenSCAD: it replaced/parallels CGAL as the fast mesh CSG backend. It is not a B-rep kernel and does not replace OCCT's CAD feature set.

#### What B-rep actually speeds up

B-rep is not universally faster; it changes which operations are cheap.

**Faster with B-rep:**

- **Repeated geometric queries** --- mass properties, distances, interference, ray casting, point classification. Analytic surfaces and topology avoid triangle-level work.
- **Face/edge/vertex picking and snapping** --- stable topological entities instead of triangle IDs.
- **Fillets, chamfers, shells, drafts, offsets** --- native operations; mesh approximations are expensive or impossible.
- **STEP/IGES import/export** --- exact surfaces, no tessellation loss.
- **Large analytic models** --- a few surfaces instead of millions of triangles; less memory and less tessellation work.
- **Assembly mates/constraints** --- axes, planes, and faces are available directly.
- **Booleans on simple analytic solids** --- exact and compact results; a hole in a plate is cheap.

**Slower or worse with B-rep:**

- **Large CSG trees** --- each exact boolean is expensive; Manifold is optimized for triangle meshes and parallelizes across many booleans. Models with hundreds or thousands of operations usually rebuild faster as meshes.
- **Non-manifold, self-intersecting, or open inputs** --- B-rep requires valid solids; healing is expensive and may fail. Manifold is more forgiving.
- **`$fn`-faceted models** --- to match OpenSCAD, primitives must become polygonal B-reps with many faces; booleans on those are slow and defeat the analytic advantage.
- **`hull()` and `minkowski()`** --- no native B-rep equivalents; you fall back to meshes.
- **Imported meshes** --- sewing a triangle soup into a valid B-rep is lossy and slow; often not worth it.
- **High-genus/complex topology** --- boolean results can explode in face count; tolerances and coincident faces cause failures.
- **Interactive preview of CSG-heavy models** --- Manifold’s rebuild latency is usually lower.

Representation is only one factor: topology, tolerance, face count, and parallelism matter more than "B-rep vs mesh" in isolation. The practical rule is **B-rep for queries and CAD features, Manifold for CSG evaluation and compatibility**.

### 6.2 Why OCCT is a poor default for arbitrary `.scad`

Using OCCT as the evaluator for arbitrary `.scad` creates fidelity problems:

- **`$fn`/`$fa`/`$fs` semantics.** SCAD primitives are faceted at evaluation. A `cylinder($fn=6)` is a hexagonal prism; a low-`$fn` sphere is a faceted solid. OCCT primitives are analytic. To match SCAD you would have to build polygonal profiles and extrude/revolve them, not call `BRepPrimAPI_Cylinder`. At high `$fn`, booleans on huge B-reps are slow.
- **`hull()` and `minkowski()`.** These are mesh operations. OCCT's API is B-rep-centric; `hull`/`minkowski` are not first-class and would need a mesh fallback.
- **`polyhedron()`, `import()`, `surface()`.** These produce meshes; OCCT can sew them into shells, but not always into valid solids.
- **Tolerance of bad input.** OpenSCAD/CGAL/Manifold tolerate many self-intersecting or non-manifold configurations; exact B-rep booleans can fail or produce unexpected topology.
- **2D/text.** `offset()`, `projection()`, and `text()` have SCAD-specific semantics; OCCT does not provide `text()` outlines.
- **`render()`.** SCAD's `render()` forces exact evaluation with a specific kernel; there is no OCCT equivalent.

If the goal is "open any `.scad` and see what OpenSCAD sees," use Manifold. If the goal is "a B-rep CAD app that serializes to `.scad`," OCCT is viable, but the language subset and the fidelity contract must be explicit.

### 6.3 Four viable architectures

> **Current direction (2026-10-28):** A (Manifold-first) is superseded. The plan is B-rep-first with OCCT, SolidCore, or Truck; exact OpenSCAD identity is out of scope. The `Shape` IR below is retained as a design sketch for the internal mesh fallback and conversion rules, not as a mandate to ship Manifold.

#### A. Manifold-first (superseded)

- Evaluate SCAD with Manifold.
- Render tessellated meshes.
- Use OCCT only later, optionally, for:
  - STEP export,
  - fillets/chamfers on selected B-rep-convertible bodies,
  - exact measurement.
- Pros: best SCAD fidelity, fastest preview, simplest semantics, Apache/MIT.
- Cons: no native fillets/STEP/topology; face/edge picking must be built on triangles or inferred.

#### B. OCCT-first

- Evaluate a B-rep-friendly subset with OCCT.
- Use mesh fallback for `hull`, `minkowski`, `polyhedron`, imports, and non-solid inputs.
- Pros: CAD-grade modeling, fillets, STEP, topology, stable face/edge IDs.
- Cons: not a faithful arbitrary `.scad` evaluator; slower; more failure modes; LGPL distribution obligations.

#### C. Dual kernel

- Manifold for "SCAD mode" / preview / compatibility.
- OCCT for "CAD mode" / B-rep features / STEP.
- Define which nodes are evaluated by which kernel and how shapes cross the boundary.
- Pros: can serve both goals.
- Cons: two geometry models, two caches, two sets of bugs; conversion between mesh and B-rep is lossy and fragile.

#### D. Hybrid representation (design sketch)

Keep both representations as first-class shape values. Existing SCAD nodes produce meshes; new NURBS/B-rep nodes produce B-reps. A B-rep shape carries a mesh cache for rendering and for operations that need triangles.

```rust
enum Shape {
    Brep(Brep),                                      // canonical
    BrepWithMesh { brep: Brep, mesh: OnceLock<Mesh> },
    Mesh(Mesh),                                      // internal fallback only
}
```

Conversion policy:

- **B-rep → mesh:** tessellate. Cheap, controlled, always available. This is the normal boundary crossing.
- **Mesh → B-rep:** explicit, lossy, best-effort (sew/heal). Only for commands like "convert to B-rep" or "fillet this mesh"; never automatic.
- **Mixed booleans:** tessellate the B-rep and use the internal mesh path. Do not sew meshes into B-reps implicitly.

Operation dispatch:

| Operation | Mesh + mesh | B-rep + B-rep | Mixed |
| --- | --- | --- | --- |
| `union` / `difference` / `intersection` | internal mesh | B-rep first; mesh fallback for mixed operands | tessellate B-rep → internal mesh |
| `translate` / `rotate` / `scale` | internal mesh | B-rep transform | transform each side |
| `hull` / `minkowski` | internal mesh | internal mesh (tessellate first) | internal mesh |
| `linear_extrude` / `rotate_extrude` | internal mesh | B-rep (preferred) | n/a |
| `fillet` / `chamfer` / `shell` | unsupported (or explicit convert) | OCCT | convert mesh → B-rep (lossy) |
| `import` STL/OBJ/3MF | mesh | n/a | n/a |
| `import` STEP/IGES | n/a | B-rep | n/a |
| `export` STL/3MF | tessellate if needed | tessellate | tessellate |
| `export` STEP | unsupported | B-rep | convert mesh → B-rep (lossy) |

Compatibility contract:

- Existing `.scad` is evaluated with the selected B-rep kernel. Mesh-only operations (`hull`, `minkowski`, mesh imports) fall back to the internal mesh path. Exact OpenSCAD output identity is not guaranteed; users who need it should use OpenSCAD.
- New features: B-rep/NURBS. They can be exact and STEP-exportable.
- Crossing: B-rep → mesh at the boundary; the result becomes a mesh for downstream SCAD operations.
- Expect differences from OpenSCAD in `$fn` faceting, `hull`/`minkowski`, and non-manifold inputs. Document them; do not promise exact identity.

Tessellation policy:

- Mesh path: `$fn/$fa/$fs` as today.
- B-rep path: a chord/angle tolerance; for display and mesh export, map `$fa/$fs` to tolerance and `$fn` to segment counts where meaningful. Document that B-rep surfaces are not controlled by `$fn` unless the feature says so.
- Cache tessellation on the shape; invalidate when parameters change.

Why this matches the stated preference: it keeps the entire existing language on the compatibility path while making B-rep/NURBS an additive feature set. The cost is a shape enum and a dispatch table; the alternative (OCCT-first) pays for it by losing `$fn`/`hull`/`minkowski` fidelity for existing code.

NURBS can be handled two ways under this model:

- **Keep the NURBS surface as B-rep** (a face/shell), and tessellate it only for rendering and mesh operations. This preserves exactness for STEP export, fillets, exact intersections, and face-level selection.
- **Tessellate immediately** and keep only the mesh. This is simpler and enough for display and 3D printing, but gives up exactness and CAD operations on the surface.

The hybrid `Shape` IR lets different language features choose: `nurbs_surface()` can default to B-rep with a mesh cache, while a display-only or print-only feature can produce a mesh directly.

Current direction: **B-rep-first**. A (Manifold-first) is no longer the plan; exact OpenSCAD identity is out of scope. D’s shape IR collapses toward a B-rep `Shape` plus an internal mesh fallback for `hull`/`minkowski`/mesh imports. The remaining choice is B (OCCT-first) versus a pure-Rust first backend (SolidCore or Truck). See [`kernels.md`](kernels.md) for the per-operation matrix.

### 6.4 Rust bindings and licences

| Crate | What it is | Version / activity | Licence | Notes |
| --- | --- | --- | --- | --- |
| `manifold-csg` / `manifold3d` | Safe Rust bindings to Manifold's C API | 0.4.1, Sep 2026 | Apache-2.0 OR MIT | f64 meshes, 2D `CrossSection`, extrude/revolve, hull, Minkowski, SDF, OBJ; wasm targets. Starts by cloning/compiling Manifold; can use a prebuilt library. |
| `opencascade-rs` (`opencascade`) | High-level Rust wrapper over OCCT using `cxx` | 0.3.0, Aug 2026; repo active Aug 2026 | LGPL-2.1 | Includes `occt-sys` and a wgpu viewer; still "major work in progress"; API in flux. |
| `occt-sys` | Static build of OCCT for Rust | 7.8.1, Aug 2026 | LGPL-2.1 | Geometry kernel + interchange only; excludes visualization; use wgpu. Static linking has LGPL implications. |
| `occt-wasm` | OCCT V8 compiled to WASM, loaded via wasmtime | 5.1.0, Oct 2026 | MIT OR Apache-2.0 for the crate; bundled OCCT remains LGPL-2.1 | No C++ toolchain; ~21 MB memory on first load; adds a WASM boundary. Convenient for prototyping. |
| `occt-rs` | Lower-level OCCT bindings | Early, small | AGPL-3.0 | Not recommended unless the project is AGPL; requires dynamic linking. |

Licensing notes:

- OCCT is LGPL-2.1 with an exception. Dynamic linking is the usual way to comply; static linking requires providing relinkable artifacts. `occt-sys` statically builds OCCT, so distribution obligations must be reviewed.
- `occt-wasm`'s crate is MIT/Apache-2.0, but it ships OCCT compiled to WASM; the bundled OCCT is LGPL-2.1, so OCCT's terms still apply to the kernel.
- Manifold is Apache-2.0; the Rust bindings are Apache-2.0 OR MIT. This is friendlier for a proprietary or mixed-licence app.
- If the app is GPL-compatible, OCCT's LGPL is manageable. If it is proprietary, get advice before static-linking OCCT.

### 6.5 Other B-rep and NURBS options

OCCT is not the only B-rep kernel, but it is the only mature open-source one with the full CAD feature set.

| Option                                                                       | Language | Licence                                         | Status                                                                                                                                  | Fit                                                                                                                                                                                                |
| ---------------------------------------------------------------------------- | -------- | ----------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **OCCT** (Open CASCADE Technology)                                           | C++      | LGPL-2.1 with exception                         | Mature; 7.8/8.x; huge feature set                                                                                                       | Default production B-rep kernel: fillets, STEP, topology, NURBS. Heavier and stricter than Manifold.                                                                                               |
| **Truck** (`ricosjp/truck`)                                                  | Rust     | Apache-2.0                                      | Active (Oct 2026); ~1.7k stars; younger than OCCT                                                                                       | Pure-Rust B-rep/NURBS, booleans (`truck-shapeops`), meshing, wgpu rendering. Attractive for a Rust-first app; fewer robustness/feature guarantees than OCCT.                                       |
| **monstertruck**                                                             | Rust     | Check crate metadata                            | Fork of Truck; 0.4.1, Sep 2026                                                                                                          | Per its docs, adds fillet/healing/STEP/assembly. Verify provenance and maintenance before depending on it.                                                                                         |
| **SolidCore**                                                                | Rust     | Apache-2.0                                      | Unit port complete (Oct 28, 2026): 263/263 units, 391,055/391,055 upstream lines, all green; integration/API still experimental (0.0.x) | Rust port of NVIDIA SMLib (usd-brep v1.0.1); primitives build and pass 9/9; external SSI corpus still has incomplete/wrong-topology cases. Evaluate it, but expect API churn and integration gaps. |
| **SMLib / USD BRep** (NVIDIA)                                                | C++      | Apache-2.0                                      | Newly open-sourced B-rep subset                                                                                                         | Upstream of SolidCore; a credible future alternative to OCCT if it matures.                                                                                                                        |
| **curvo**                                                                    | Rust     | MIT                                             | Active; NURBS library, not a solid kernel                                                                                               | NURBS curves/surfaces, extrude/loft/sweep/revolve, tessellation, some NURBS booleans. Good for "native NURBS" without a full B-rep kernel.                                                         |
| **CGAL**                                                                     | C++      | LGPL/GPL (package-dependent)                    | Mature; exact arithmetic                                                                                                                | Nef polyhedra B-rep, exact booleans, non-manifold geometry. Not a CAD feature kernel (no fillets/STEP); heavy; slower than commercial kernels in published benchmarks.                             |
| **OpenNURBS**                                                                | C++      | Permissive (no copyright/copyleft restrictions) | Mature; 3DM I/O + NURBS evaluation                                                                                                      | Not a solid kernel: no booleans, tessellation, intersections, or mass properties. Useful for NURBS/3DM interchange only.                                                                           |
| **SolveSpace**                                                               | C++      | GPL-3.0                                         | Mature small CAD                                                                                                                        | Own NURBS kernel + constraint solver; good reference for sketching/constraints, not a general B-rep kernel.                                                                                        |
| **Fornjot**                                                                  | Rust     | ---                                             | Archived/shut down (2026)                                                                                                               | Do not use.                                                                                                                                                                                        |
| **Commercial**: Parasolid, ACIS, CGM, Autodesk ShapeManager, C3D, ODA Kernel | C++      | Proprietary                                     | Mature                                                                                                                                  | Robust and fast, used by major CAD products; expensive and closed. Parasolid (Siemens) and ACIS (Spatial/Dassault) are the usual benchmarks.                                                       |

Recommendation:

- **Production now:** OCCT. It is the most complete open-source B-rep kernel, with the bindings and documentation to match.
- **Pure-Rust bets:** Truck and SolidCore. Truck is the more established B-rep/NURBS project; SolidCore’s unit port is now complete, but its API is still 0.0.x and integration/operation-level validation is still in progress. curvo is the pragmatic choice if you only need NURBS surfaces and can tessellate immediately.
- **Watch, don’t depend for production yet:** SolidCore. "100% ported" is unit-level: every upstream unit is ported and passes its difftest, not every end-to-end model. Its own README keeps the crate at 0.0.x until it is usable end to end, and the external SSI corpus still shows incomplete/wrong-topology cases.
- **Do not use:** Fornjot (shut down).
- **Commercial kernels:** only if B-rep robustness is the product and the licensing budget exists.

### 6.6 Native fast paths for common library constructs

A large share of real-world OpenSCAD slowness comes from library idioms that emulate curved solids with mesh operations: BOSL2's `cuboid(rounding=...)`, Round-Anything's `round3d`, `minkowski()`-based fillets, high-`$fn` extrusions, and nested booleans. These are pathological for mesh CSG: the kernel does a lot of work to approximate a shape that has a compact analytic form.

The opposite approach is to recognize the idiom and construct the exact B-rep directly. A rounded box, for example, is the Minkowski sum of a box and a sphere: 6 planar faces, 12 cylindrical edge patches, and 8 spherical corner patches for the all-edges case. OCCT can build that with a fillet on the box edges, or with a dedicated constructor. It is both faster and exact.

Use **analytic B-rep** (planes, cylinders, spheres, tori) for these constructs, not NURBS. NURBS is for freeform surfaces; a rounded box does not need it, and analytic faces are cheaper and more robust.

#### Mechanism: evaluator-level intrinsics, not source rewriting

Implement a library-shim layer in the evaluator:

- Resolve a module call to its declaration.
- If the declaration comes from a known library file (path + content hash + version) and the call matches a supported signature, evaluate the native implementation.
- Otherwise, fall back to the original module.
- Make it opt-in and overridable (`--native-shims`, a pragma, or a per-file setting).

Do not rewrite the user's `.scad` and do not replace the whole library module blindly. BOSL2 modules carry attachment/anchor/spin/orient behavior and may call `children()`, `$parent_modules`, tags, and other protocol state. Replacing the outer module breaks composition. Prefer one of:

1. Shim the geometry-producing branch inside the module while preserving BOSL2's `attachable()` wrapper. BOSL2's `cuboid` is not a thin `_cuboid` wrapper: it is one module with nested helpers (`corner_shape`, `xtcyl`, `ytcyl`, `tsphere`, ...) and an `attachable()` call. A native override must reproduce the wrapper or patch only the geometry branch.
2. Shim the outer module but implement the same attachment protocol and `children()` semantics.
3. Provide native modules under new names and rewrite calls only in a controlled source transform.

Option 1 is the safest and preserves the most behavior.

#### Candidate constructs, in order of expected payoff

| Construct                                         | Native representation                                                              | Difficulty                      |
| ------------------------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------- |
| `cuboid(rounding=r)` with all edges               | Analytic rounded box (planes + cylinders + spheres) or OCCT fillet on all 12 edges | Low                             |
| `cuboid(chamfer=c)` with all edges                | Chamfered box (planes + planar/conical patches)                                    | Low–medium                      |
| `cuboid(rounding=..., edges=...)` subset          | OCCT fillet on selected edges                                                      | Medium; fillet-chain robustness |
| 2D `rect(rounding=r)` + `linear_extrude`          | 2D offset/fillet + extrude                                                         | Low                             |
| `cyl(rounding=...)`, `sphere`, `torus`            | Analytic primitives                                                                | Low                             |
| `slot`, `prismoid`, `wedge`                       | Analytic B-rep                                                                     | Low–medium                      |
| `round3d` / `minkowski`-based fillets             | B-rep fillet / offset                                                              | Medium–high                     |
| Threads, gears, VNFs, attachment-heavy constructs | Keep mesh/library path                                                             | High; low payoff initially      |

#### Semantics and fidelity

- Match the outer envelope and edge radii, not the triangulation. Tessellate the B-rep at `$fn/$fa/$fs` for display and mesh export; keep the exact B-rep for STEP, measurement, and further B-rep operations.
- BOSL2's `$fn`-driven rounding is faceted; a native analytic B-rep is smooth. At low `$fn`, the rendered result may differ slightly. This is acceptable under "approximately equivalent", but document it.
- Parameter semantics matter: `size` vs `size2`, `rounding` vs `chamfer`, `edges`/`except`, `trim`, `anchor`, `spin`, `orient`, attachments, `children()`. Support a documented subset and fall back for everything else.
- Start with the common case: a `size` vector, positive `rounding` or `chamfer`, `edges=EDGES_ALL`, `trimcorners=true`, no `teardrop`/`clip_angle`, and no `p1`/`p2`. Fall back for edge subsets, negative rounding/chamfer (external fillets), teardrop/clip-angle, and the rest.
- Match BOSL2's `segs()`/`$fn` faceting when tessellating, so the native shape looks like the library result at the same `$fn`.
- Pin the shim to library versions (content hash) and test against the upstream test suite or a corpus of models. A BOSL2 upgrade can change signatures and internals.

#### Performance methodology

Do not rely on synthetic benchmarks. Build a corpus of real models (Thingiverse/Printables `.scad` files, BOSL2 examples, the OpenSCAD test suite) and measure:

- parse/eval time, CSG time, tessellation time, total rebuild;
- memory high-water mark;
- failure rate and visual diff;
- cold vs cached runs.

Expect the largest wins where the original idiom is `minkowski`/`hull`/high-`$fn` + booleans. Expect possible losses on trivial shapes where OCCT kernel overhead dominates. Measure before expanding the shim set.

#### Recommendation

Implement two or three high-value intrinsics first --- `cuboid(rounding=...)`, `cuboid(chamfer=...)`, and 2D `rect(rounding=...)` + extrude --- behind a versioned shim with a strict fallback. Measure on real models. Expand only where the corpus shows a win.

### 6.7 A common kernel API and backend swapping

Short answer: **yes** for experimentation and for keeping kernels isolated; **no** if you expect one trait to make every kernel interchangeable with identical results.

Two different things are being conflated:

1. **Session/document backend** --- the whole model is evaluated by one kernel. This is easy to support: a `Kernel` trait object, opaque `ShapeId`s, and a backend selector in settings. Good for benchmarks ("compare OCCT vs SolidCore on this file") and for letting a user choose "pure Rust" vs "OCCT".
2. **Per-node hybrid** --- B-rep and internal mesh shapes coexist in one model. This is architecture D in §6.3. It needs `ShapeRef { kernel, id }` plus explicit conversion rules; it is not just a swappable backend.

#### Design around capabilities, not a lowest-common-denominator surface

A minimal common API:

```rust
trait Kernel {
    fn capabilities(&self) -> Capabilities;
    fn primitive(&mut self, p: Primitive) -> Result<ShapeId>;
    fn boolean(&mut self, op: BoolOp, a: ShapeId, b: ShapeId) -> Result<ShapeId>;
    fn transform(&mut self, s: ShapeId, t: Transform) -> Result<ShapeId>;
    fn tessellate(&self, s: ShapeId, tol: TessTolerance) -> Result<Mesh>;
    fn mass_properties(&self, s: ShapeId) -> Result<MassProperties>;
}
```

Use `ShapeId` (an arena handle) rather than an associated `Shape` type. That keeps the trait object-safe, so the app can hold `Box<dyn Kernel>` and switch backends at runtime. A vtable call per operation is negligible next to geometry work. The cost is that shape handles are kernel-local and invalid after a backend switch; the app must invalidate caches, selection IDs, and tessellation, then re-evaluate.

Optional capability traits keep the core small:

- `FilletKernel` --- fillet, chamfer, shell, draft, offset
- `NurbsKernel` --- NURBS curves/surfaces, STEP
- `TopologyKernel` --- faces/edges/vertices, stable IDs, picking
- `StepIo` --- import/export

Expose their presence through `Capabilities` so the UI can grey out unsupported operations. Do not put kernel-specific shape types in the UI or document model.

#### Rough capability matrix

The detailed, per-operation matrix is in [`kernels.md`](kernels.md). Summary:

| Capability         | OCCT            | SolidCore                      | Truck (upstream / fork)  |
| ------------------ | --------------- | ------------------------------ | ------------------------ |
| Representation     | B-rep           | B-rep                          | B-rep/NURBS              |
| Booleans           | Yes             | Yes                            | Yes                      |
| Fillet/chamfer     | Yes             | Yes                            | Upstream no; fork yes    |
| NURBS              | Yes             | Yes                            | Yes                      |
| STEP               | Yes             | Not in scope / verify          | Yes                      |
| Face/edge topology | Yes             | Yes                            | Yes                      |
| wasm               | Via `occt-wasm` | Yes (`wasm32-unknown-unknown`) | Via `truck-js` (verify)  |
| Licence            | LGPL-2.1        | Apache-2.0                     | Apache-2.0 (fork verify) |
| API maturity       | Mature          | 0.0.x, integration in progress | Young                    |

Manifold is not a candidate backend; it may appear only as an internal mesh fallback for `hull`/`minkowski`/mesh imports.

#### Swapping through the UI

Technically easy; semantically dangerous. A few rules:

- Persist the backend choice in workspace/session settings, never in the `.scad`.
- Treat a switch as "close and re-evaluate with another kernel", not a seamless hot-swap. Warn that geometry, tessellation, topology IDs, and selection may change.
- Check capabilities before evaluating. If the file uses a feature the new backend lacks (fillets, STEP import, NURBS), offer a fallback or refuse; do not silently produce different geometry.
- Keep "compare kernels" as an advanced/debug mode. It is excellent for finding kernel bugs and for benchmarking; it is a support burden as a primary user workflow.
- Do not bundle every backend in every build by default. Use feature flags and separate builds; OCCT adds a C++ toolchain, build time, and LGPL obligations; the Rust kernels add API churn.

#### Kernel Recommendation

Start with **OCCT plus one pure-Rust backend** (SolidCore or Truck), not all three. Use OCCT as the production fallback and conformance oracle; use the pure-Rust backend as the strategic path. Write `kernel-api` against those two real implementations; add the third behind a feature flag for benchmarking. If the product must ship only two, SolidCore + Truck is viable, but keep OCCT in CI as the oracle and as an opt-in feature for STEP/fillet problems. The UI selector is a thin layer over the same `Kernel`/`Capabilities` API.

## 7. Language and runtime

### 7.1 Rust, not Rust + Zig

Recommendation: Rust only, with OCCT as the optional C++ backend.

- OCCT is C++; the pure-Rust backends (SolidCore, Truck) avoid a C++ toolchain entirely. If the internal mesh fallback uses Manifold, that adds C++ only for `mesh-ops`.
- Adding Zig adds a third toolchain, a third CI matrix, and a second FFI boundary. The parser is unlikely to be the bottleneck; geometry evaluation and tessellation dominate.
- Rust has the ecosystem this app needs: `rowan`/`cstree`, `logos`, `ropey`, `wgpu`, `egui`/`iced`, `serde`, `rayon`, `cxx`, `bindgen`.
- If parser performance ever matters, a hand-written lexer + Pratt/recursive-descent parser in Rust is fast enough. Measure before adding a language.
- `cxx` is the right bridge for OCCT's C++ API. If `mesh-ops` uses Manifold, its C API is wrapped by `manifold-csg`.

### 7.2 Process/thread model

- Keep the core (parser, evaluator, kernel) independent of the UI.
- Run parse/evaluate on a worker thread; keep the UI responsive.
- OCCT can abort the process on bad input. For a shipping app, consider a dedicated kernel process with IPC, so a crash does not lose the document. Start with a thread if that is too much for v1, but design the boundary so it can become a process.
- Cache shapes aggressively; use content hashes, not pointer identity.

## 8. Rendering

`wgpu` is the de facto cross-platform GPU abstraction for Rust (Vulkan/Metal/DX12/WebGPU). It is not a scene graph or a CAD renderer; you still write the renderer.

Recommended pipeline:

1. Kernel produces a tessellated B-rep (OCCT `BRepMesh_IncrementalMesh`, SolidCore/Truck meshing) or a mesh from `mesh-ops`.
2. Upload vertex/index buffers to wgpu; keep per-node/per-face IDs.
3. Render with a depth buffer, MSAA, and a simple PBR-ish material.
4. Selection/picking:
   - render an ID buffer (`R32Uint`) with body/face IDs and read back the pixel under the cursor; or
   - CPU raycast against a BVH of triangles (`parry3d`, `bvh`).
   - ID buffer is simpler and fast; face/edge picking needs topology IDs from the B-rep kernel.
5. Gizmos:
   - `transform-gizmo` (0.11.0, Aug 2026) provides a 3D transformation gizmo; check its renderer integration.
   - Or implement ray-plane/ray-axis intersection and draw overlays.
6. Overlays: selection outlines, ghost cutters, section planes, measurements.
7. Performance: frustum culling, instancing for repeated bodies, tessellation LOD, and a tessellation cache keyed by node hash.

For a mesh kernel, "faces" are triangles. If face-level selection, fillets, or STEP are core, that is an argument for OCCT.

## 9. UI toolkit options

| Toolkit            | Model                  | wgpu viewport                         | Strengths                                                                                       | Weaknesses                                                                                                                           | CAD examples                      |
| ------------------ | ---------------------- | ------------------------------------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------- |
| **egui / eframe**  | Immediate mode         | Native (`egui-wgpu` / paint callback) | Fastest to build; simple state; docking via `egui_dock`; good for tools and debug UI; pure Rust | Immediate-mode layout cost at scale; weaker accessibility, text editing, and native menu integration; tree virtualization needs care | SolidCraft, ConstructCAD, BREP    |
| **iced**           | Retained / Elm         | Native (`iced_wgpu`)                  | Clean architecture; good performance; used by a real CAD app; pure Rust                         | More boilerplate; smaller widget set; custom GPU widget integration is more work                                                     | Open CAD Studio                   |
| **Slint**          | Declarative            | Texture bridge / custom item          | Polished native-looking UI; good performance; designer-friendly                                 | CAD-specific widgets (tree, dock) are limited; licence depends on distribution; custom GPU integration                               | Fewer CAD examples                |
| **Qt / QML**       | Retained               | `QQuickFramebufferObject` / native    | Best CAD UI widgets (tree, dock, menus, accessibility); OCCT has Qt viewer samples; mature      | Not pure Rust; `cxx-qt`/QML; licensing (LGPL/commercial); heavier                                                                    | FreeCAD, many commercial CAD apps |
| **Tauri + web UI** | Web (React/Svelte/...) | WebGPU canvas or wgpu surface         | Best UI iteration and flexibility; easy tree/property panels; web-dev skills transfer           | IPC boundary; native feel/performance; packaging; two runtimes                                                                       | Several web CAD tools             |
| **GTK4**           | Retained               | Custom GL/Vulkan area                 | Native Linux integration                                                                        | Weak cross-platform GPU story; not ideal for CAD                                                                                     | Few                               |

Recommendation: **egui + wgpu for v1**. It is the lowest-friction path to a working tree + properties + 3D viewport, and real Rust CAD projects use it. Keep the core UI-agnostic behind a command/event boundary. If the product later needs a polished, accessible, native-feeling UI, move the shell to Qt/QML or a web front end (Tauri) without rewriting the core.

If the UI is the product and you want a conventional retained widget tree from day one, choose **Qt/QML** and accept writing UI in C++/QML. If you want web UI skills and rapid iteration, choose **Tauri**. Do not choose a toolkit before deciding how much of the app is text editing versus direct manipulation; text editing quality is a common weak spot in Rust GUI toolkits.

## 10. Proposed crate layout

```text
crates/
  scad-syntax/      lexer, lossless CST (rowan/cstree), parser, formatter, TextEdit
  scad-hir/         typed AST, name resolution, scopes, module/function symbols
  scad-eval/        evaluator, instantiated model tree, stable node IDs, caches
  scad-intrinsics/  native fast paths for known library constructs (BOSL2, etc.)
  geom/             kernel-neutral mesh/types, transforms, IDs, measurement
  mesh-ops/         optional internal mesh fallback (hull, minkowski, mesh imports)
  kernel-api/       Kernel trait, ShapeId, Capabilities, neutral tessellation types
  kernel-occt/      OCCT adapter (feature-gated)
  kernel-solidcore/ SolidCore adapter (feature-gated)
  kernel-truck/     Truck adapter (feature-gated, experimental)
  render/           wgpu renderer, picking, gizmos, overlays
  ui/               egui (or iced) shell: tree, properties, inspector
  app/              file watching, autosave, settings, session state
```

Key boundaries:

- `scad-syntax`, `scad-hir`, `scad-eval`, `geom` have no UI dependencies.
- `kernel-api` owns the trait, `ShapeId`, capabilities, and neutral tessellation types; `kernel-*` implements it. Start with OCCT plus one pure-Rust backend; add the rest behind feature flags.
- Manifold, if used at all, lives behind `mesh-ops` and is not a selectable kernel.
- `scad-intrinsics` is versioned per library and must always be able to fall back to the original module.
- `kernel-*` implements a narrow trait; do not leak kernel types into the UI.
- `render` consumes meshes + IDs; it does not know about SCAD.
- `ui` issues commands and receives events; it does not own evaluation state.
- `app` owns the document, the worker thread, and the session/inspection state.

## 11. Open questions

1. **Fidelity contract.** Exact OpenSCAD output identity is out of scope; "approximately equivalent in rendering and topology" is the bar. Which differences are acceptable (`$fn` faceting, `hull`/`minkowski`, non-manifold inputs), and which would be regressions?
2. **Timeline semantics.** Is "timeline" a Fusion-style linear feature history, an evaluation-step debugger, or a subtree inspector? Only the latter two are natural for SCAD.
3. **Editing scope.** Is the tree read-only plus property edits, or full structural editing (add/remove/reorder nodes)? Structural editing needs a robust CST edit API and a source-formatting policy.
4. **Constraints.** Formula-only relationships (v1) or a real solver? If a solver, how are solved values written back without destroying formulas?
5. **Kernel licensing.** Is LGPL acceptable for OCCT? If not, SolidCore and Truck are Apache-2.0; OCCT can remain a CI-only oracle rather than a shipped backend.
6. **Web target.** SolidCore is wasm-native; Truck has `truck-js`; OCCT runs via `occt-wasm`. Verify actual browser builds and memory before committing to a wasm target.
7. **File watching and external edits.** What happens when the `.scad` file changes outside the app while there are unsaved tree edits? BelfrySCAD's "never silently overwrite" policy is a good default.
8. **Shape IR.** Is `Shape { Brep | BrepWithMesh | MeshFallback }` sufficient, or do we need a richer representation (e.g. implicit/SDF, subdivision, point clouds)? How is mesh → B-rep conversion exposed in the UI?
9. **B-rep kernel bet.** OCCT as production/oracle plus SolidCore/Truck as the pure-Rust path, or ship only SolidCore + Truck? This is partly a licensing and maturity decision, not only a technical one.
10. **Native shims.** Which libraries and constructs get intrinsics, how are they versioned, and how is fallback tested? The BOSL2 attachment protocol is the main compatibility risk.
11. **Backend swap semantics.** Is kernel selection per document/session or per node? What happens to unsupported features, cached geometry, and selection IDs when the user switches backends?
12. **Mesh-only operations.** If Manifold is not included, how are `hull`, `minkowski`, and mesh imports handled: an internal mesh utility, a custom implementation, or "fallback to OpenSCAD"?
13. **Packaging.** Feature-gated crates in one binary, a `kernel-host` sidecar process, or separate builds per backend? The sidecar is the strongest option for OCCT crash isolation and LGPL replacement.

## 12. Sources

Accessed 2026-10-28 unless noted.

- OwlCAD, "An OpenSCAD Alternative That Opens Your .scad Files" --- <https://owlcad.com/openscad-alternative>
- OwlCAD, features --- <https://owlcad.com/features>
- KherveCAD, Zenodo record --- <https://zenodo.org/records/23305286>
- KherveCAD, product page --- <https://khervetools.com/tools/khervecad/>
- 3dscad, GitHub --- <https://github.com/42ne/3dscad>
- BelfrySCAD, GitHub --- <https://github.com/BelfrySCAD/BelfrySCAD>
- BelfrySCAD wiki, Tour --- <https://github-wiki-see.page/m/BelfrySCAD/BelfrySCAD/wiki/Tour>
- BelfrySCAD wiki, Rendering --- <https://github-wiki-see.page/m/BelfrySCAD/BelfrySCAD/wiki/Rendering>
- Cascade Studio, GitHub --- <https://github.com/zalo/CascadeStudio>
- OpenSCAD Graph Editor, GitHub --- <https://github.com/derkork/openscad-graph-editor>
- FreeCAD wiki, OpenSCAD Workbench --- <https://wiki.freecad.org/OpenSCAD_Workbench>
- J. F. Gonzalez et al., "Introducing Bidirectional Programming in Constructive Solid Geometry-Based CAD," arXiv:2408.01801 --- <https://arxiv.org/abs/2408.01801>
- SolidCraft, GitHub --- <https://github.com/tangency-labs/solidcraft>
- SolidCore, GitHub --- <https://github.com/tangency-labs/solidcore>
- Open CAD Studio (OpenAEC) --- <https://open-aec.com/es/open-cad-studio/>
- BREP crate --- <https://lib.rs/crates/brep>
- ConstructCAD, GitHub --- <https://github.com/francisreboucas/constructcad>
- Manifold, upstream --- <https://github.com/elalish/manifold>
- `manifold-csg` crate --- <https://crates.io/crates/manifold-csg>
- `manifold3d` crate --- <https://crates.io/crates/manifold3d>
- `opencascade-rs` --- <https://github.com/bschwind/opencascade-rs>
- `occt-sys` --- <https://lib.rs/crates/occt-sys>
- `occt-wasm` --- <https://crates.io/crates/occt-wasm>
- `occt-rs` --- <https://github.com/occt-rs/occt-rs>
- `rowan` --- <https://github.com/rust-lang/rowan>
- `cstree` --- <https://docs.rs/cstree>
- `wgpu` --- <https://wgpu.rs/>
- `egui` --- <https://github.com/emilk/egui>
- `iced` --- <https://github.com/iced-rs/iced>
- Slint --- <https://slint.dev/>
- `transform-gizmo` --- <https://crates.io/crates/transform-gizmo>
- OCCT (Open CASCADE Technology) --- <https://dev.opencascade.org/>
- Truck --- <https://github.com/ricosjp/truck>
- Truck user book --- <https://truckkernel.com/>
- `monstertruck` crate --- <https://crates.io/crates/monstertruck>
- curvo --- <https://github.com/mattatz/curvo>
- Fornjot (archived) --- <https://github.com/hannobraun/fornjot>
- CGAL 3D Nef polyhedra --- <https://doc.cgal.org/latest/Nef_3/>
- Hachenberger, Kettner, Mehlhorn, "Boolean operations on 3D selective Nef complexes" --- <https://www.sciencedirect.com/science/article/pii/S0925772107000181>
- Hybrid-representation boolean operations paper --- <https://publications.rwth-aachen.de/record/1016540/files/1016540.pdf>
- OpenNURBS --- <https://developer.rhino3d.com/guides/opennurbs/what-is-opennurbs/>
- SolveSpace --- <https://solvespace.com/>
- Parasolid (Siemens) --- <https://www.sw.siemens.com/en-US/plm-components/parasolid/>
- ACIS (Spatial) --- <https://www.spatial.com/products/3d-acis-modeling>
- C3D Labs --- <https://c3dlabs.com/>
- Open Design Alliance --- <https://opendesign.com/>
