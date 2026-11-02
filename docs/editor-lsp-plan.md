# Editor, LSP, and dependency-upgrade plan

Last updated: 2026-11-02
Status: proposed. No editor or LSP code exists yet. The viewer has since gained
the analytic compute preview and render-on-demand, which Phase 0 must migrate
and Phases 1--2 must not regress.

This plan replaces the plain `egui::TextEdit` source pane with a rowan-backed
editor, then adds a standalone `brepscad-lsp` server and, last, an in-app LSP
client. It also schedules the dependency upgrade that should happen before the
editor work starts. A future Floem switchover is described at the end but is
explicitly **not** part of this plan.

## Current baseline (2026-11-02)

- `crates/viewer` uses eframe 0.32.3 / wgpu 25.0.2. The workspace manifest says
  `rust-version = "1.85"` while `rust-toolchain.toml` pins 1.95.
- The source pane is still a plain `egui::TextEdit` in `app.rs`, and the left
  panel is one `ScrollArea` containing source, error, shapes and properties.
- The 3D view has two wgpu paths: the mesh rasteriser in `renderer.rs` and the
  analytic compute preview in `analytic.rs`. `RedrawGate` redraws only when the
  camera, target size, mesh, scene or analytic mode changes.
- `brepscad-syntax` already provides the lossless CST, tolerant parser and
  `SyntaxError { message, offset }`.
- `brepscad-eval` has a mostly string-only `EvalError` (about 75 construction
  sites) and no shared built-in metadata table.

## Decisions in this plan

- Keep `eframe`/`egui` and the existing `wgpu` 3D viewport. Do not move to
  Floem now.
- Upgrade to `eframe` 0.37 / `wgpu` 30 first, while the codebase is small.
- Keep the editor implementation in `crates/viewer/src/editor/` initially.
  Extract a crate only if it needs to be reused outside the viewer.
- Reuse `brepscad-syntax` for highlighting and diagnostics rather than adding
  a second OpenSCAD parser or a keyword-list highlighter.
- Use egui 0.37's built-in `CompletionPopup` for the popup, keyboard handling
  and focus; keep candidate collection as a pure function. This is another
  reason to take the Phase 0 bump before Phase 1.
- Do not add `egui_code_editor`: its highlighting is keyword-driven and its
  completion is dictionary-based, so it would duplicate rather than use
  `brepscad-syntax`.
- Add a conventional in-window menu bar (File/Edit/View/Help) and route menu
  actions and keyboard shortcuts through one editor command layer. Remove the
  permanently displayed Open/Save/Save As buttons.
- Use egui's `TextEditUndoer` for v1 undo/redo. Add explicit transaction
  boundaries only if parameter-panel edits group badly.
- Put LSP in a separate `crates/scad-lsp` binary. The in-app client comes last
  and always has a local fallback.
- Do not add multi-cursor or code folding to the egui editor. Both require
  replacing `TextEdit` with a custom widget; that is a Floem-sized project.

Order of work:

```text
Phase 0: version bump
Phase 1: rowan editor
Phase 2: menu bar and edit commands
Phase 3: structured diagnostics
Phase 4: brepscad-lsp server
Phase 5: in-app LSP client
Phase 6: polish and deferred items, continuously
```

Phase 0 is independent and can land immediately. Phase 1 only needs Phase 0.
Phase 2 depends on the `CodeEditor` wrapper and shared edit helper from
Phase 1 and can proceed in parallel with Phase 3. Phase 3 and Phase 4 can
overlap, because Phase 4 can start with parse errors alone. Phase 5 needs a
usable server.

## Non-goals

- Replacing `eframe`/`egui` or the `wgpu` renderer.
- Multi-cursor, code folding, a minimap, or Vim modal editing in the egui
  editor.
- Native OS menu bars or platform-specific menu integration. v1 uses an
  in-window egui menu bar on every platform.
- Exact OpenSCAD output identity or full OpenSCAD language coverage.
- Evaluating geometry inside the language server on every keystroke.
- Session persistence, export, or a document format other than `.scad`.

## Rejected alternatives

- **`egui_code_editor` 0.4.2.** It requires egui 0.36, and it provides keyword
  highlighting and dictionary completion rather than CST-aware highlighting or
  LSP. It would be a second highlighter to maintain.
- **Switching to Floem now.** The wgpu-in-Floem integration is unproven and the
  shell rewrite is larger than the editor work. See the future section.
- **Zed/gpui.** `gpui` is published, but Zed's editor crate is
  GPL-3.0-or-later and coupled to Zed's workspace; it is not a drop-in
  library.

## Phase 0 --- dependency and toolchain upgrade

### Target versions

The lockfile already resolves most dependencies to their latest patch
releases. The real changes are `eframe`, `egui`, `wgpu`, and possibly `rowan`.

| Crate | Manifest | Locked | Target | Action |
| --- | --- | --- | --- | --- |
| Rust toolchain | 1.95.0 | 1.95.0 | 1.95.0 | Keep; `rust-toolchain.toml` already pins it. This is exactly the eframe 0.37 MSRV. |
| Workspace `rust-version` | 1.85 | 1.85 | 1.95 | Raise it to eframe 0.37's MSRV. |
| `eframe` | 0.32 | 0.32.3 | 0.37.0 | Bump; brings `egui` 0.37. |
| `wgpu` | 25 | 25.0.2 | 30.0.1 | Bump; API migration required. |
| `rowan` | 0.16 | 0.16.1 | 0.17.0 | Bump if tests pass; skip 0.18 alphas. |
| `rfd` | 0.17 | 0.17.2 | 0.17.2 | No change. |
| `glam` | 0.34 | 0.34.1 | 0.34.1 | No change. |
| `bytemuck` | 1 | 1.25.2 | 1.25.2 | No change. |
| `thiserror` | 2 | 2.0.21 | 2.0.21 | No change. |
| `anyhow` | 1 | 1.0.104 | 1.0.104 | No change. |
| `truck-modeling` | 0.6 | 0.6.0 | 0.6.0 | No change. |
| `truck-meshalgo` | 0.4 | 0.4.0 | 0.4.0 | No change. |
| `truck-shapeops` | 0.4 | 0.4.0 | 0.4.0 | No change; vendored patch stays. |
| `clipper2-rust` | 1.2 | 1.2.0 | 1.2.0 | No change. |

The toolchain is pinned but the manifest's MSRV is not: raise
`[workspace.package] rust-version` to 1.95 (or override it in `crates/viewer`)
before adding eframe 0.37; otherwise Cargo's MSRV check no longer describes the
real minimum.

Do not bump Rust to 1.98 unless the project adopts Floem or another dependency
requires it. `eframe` 0.37 needs 1.95 and `wgpu` 30 needs 1.87.

After editing the manifests, run `cargo update` and review the lockfile diff.
Keep unrelated dependency churn out of the version-bump change. egui 0.32 to
0.37 may also change viewer widget APIs (for example scroll and viewport
configuration); keep those edits mechanical and separate from the editor work.
After the bump, compile-check the egui APIs this plan relies on
(`CompletionPopup`, `TextEditState::undoer`, `MenuBar`,
`ViewportCommand::CancelClose`, `TextEditOutput`) before starting Phase 1;
0.32 and 0.37 differ in more than the version number.

### wgpu 25 -> 30 migration checklist

The migration is still bounded, but no longer one file: `renderer.rs` is about
560 lines with roughly 86 `wgpu::` references, and `analytic.rs` adds about 290
lines with 60 more for the compute preview. Both use the same pipeline-layout
and buffer APIs, so the compiler will find most of the rest. Known changes from
the wgpu 30 changelog:

- `PipelineLayoutDescriptor::bind_group_layouts` is now
  `&[Option<&BindGroupLayout>]`. Pass `&[Some(&bind_group_layout)]`.
- `PipelineLayoutDescriptor` has a new `immediate_size: u32` field. Set `0`.
- `VertexState::buffers` is now `&[Option<VertexBufferLayout>]`. Pass
  `&[Some(Vertex::layout())]`.
- `RenderPipelineDescriptor::multiview` was renamed to `multiview_mask`. Set
  `None`.
- `RenderPassColorAttachment` has a new `depth_slice: Option<u32>` field. Set
  `None`.
- `DepthStencilState::depth_write_enabled` and `depth_compare` are now
  `Option`s: pass `Some(true)` and `Some(CompareFunction::Less)`. This is a
  compile error, not just a recheck.
- The analytic compute pipeline also builds a `PipelineLayoutDescriptor`, so it
  needs the same `&[Some(&layout)]` and `immediate_size: 0` changes. Revalidate
  both `shader.wgsl` and `preview.wgsl`.
- Empty vertex/index buffers are a real case when evaluation fails or produces
  no geometry. wgpu 30 still rejects zero-size buffer slices in bindings and
  `set_vertex_buffer`/`set_index_buffer`; use a one-element placeholder or skip
  the draw when the mesh is empty.
- `eframe` owns the surface configuration; the new `SurfaceConfiguration`
  `color_space` field is not our code. Recheck the eframe 0.37 feature list
  (`default_fonts`, `wayland`, `wgpu`, `x11`) and make the direct `wgpu`
  dependency match the version eframe re-exports.
- The mesh shader has only float varyings and the compute shader has none, so
  the new explicit `@interpolate(flat)` requirement does not apply. Recompile
  both shaders anyway; naga validation is the source of truth.
- `egui_wgpu::Renderer::register_native_texture` and
  `update_egui_texture_from_wgpu_texture` still exist in `egui-wgpu` 0.37, and
  `eframe::egui_wgpu` is still re-exported. The offscreen texture path should
  be a version bump, not a redesign.

### Phase 0 acceptance

- `cargo fmt --all --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes.
- The viewer builds and renders `assets/sample.scad` with the same framing,
  colors, and depth behavior as before.
- The first frame still renders through the on-demand redraw gate, and idle CPU
  stays near zero after that frame.
- The corpus parse baseline is unchanged:
  `scripts/check-scad-corpus.sh --parse-only --baseline corpus/baseline-parse.tsv .corpus`.
- The CLI smoke test still prints the expected mesh counts for
  `assets/sample.scad`.

Do not regenerate corpus baselines for a dependency-only change.

## Phase 1 --- rowan-based editor

### Module layout

Keep this inside the viewer for now:

```text
crates/viewer/src/editor/
  mod.rs          CodeEditor wrapper and state
  highlight.rs    rowan token stream -> egui::text::LayoutJob
  gutter.rs       line numbers, current line, diagnostic markers
  diagnostics.rs  parse errors -> underlined spans
  completion.rs   context detection and candidate suggestions
  keymap.rs       editor shortcuts and indentation commands
  theme.rs        token colors and editor metrics
```

Tests live next to the pure functions. The UI shell is not unit-tested; the
headless screenshot script remains the visual smoke test.

### Highlighting

- Build the `LayoutJob` from `brepscad_syntax::parse(source).syntax()`.
- Walk `descendants_with_tokens()` and append one `LayoutJob` section per
  token. Preserve all text, including trivia, so offsets and cursor placement
  remain exact.
- Classify tokens by `SyntaxKind`; identifiers need text or parent context:
  - comments: muted italic
  - strings and numbers: distinct colors
  - `true`, `false`, `undef`: literal color
  - `module`, `function`, `for`, `if`, `else`, `let`, `each`, `include`,
    `use`, `echo`, `assert`: keyword color when the token is an `IDENT` with
    that text
  - identifiers in declaration position (`MODULE_DECLARATION`,
    `FUNCTION_DECLARATION`, `PARAM`) can use a declaration color
  - operators and punctuation: default or muted
- Cache the parse and the `LayoutJob` keyed by a source revision counter. The
  layouter runs every frame; do not reparse an unchanged buffer twice.
- The layouter receives `&dyn TextBuffer`; use `as_str()` and convert char
  indices to byte offsets where needed.

### Gutter and current line

- Replace the bare `TextEdit` with a `CodeEditor::show(ui, &mut source)` that
  wraps `CompletionPopup::show` and returns a small output exposing both the
  `TextEditOutput` (gutter, cursor, response) and `accepted` (completion
  insertion). A bare `TextEditOutput` is no longer enough once the popup is in
  use.
- Reserve a fixed-width gutter and paint line numbers using the `Galley` row
  positions from the `TextEditOutput`. This keeps the gutter aligned when a
  long line wraps, without synchronizing two scroll areas.
- Paint the current line number in a brighter color.
- Paint a small error marker in the gutter for each diagnostic row.
- Give the editor its own `ScrollArea`; the surrounding sections get
  independent scroll areas and resizable dividers (see "Left panel layout"
  below). Keep the Ctrl+Enter evaluate shortcut.

### Diagnostics in the editor

- Parse errors already carry an `offset`. Convert each to a byte range and add
  a red underline through `TextFormat::underline`.
- Show evaluator errors in the existing red label until Phase 3 gives them
  spans. Do not underline the whole file for a string-only evaluator error.
- Keep the diagnostics list ordered and deduplicated by range.

### Completion

- Detect the completion context from the source before the cursor:
  - suppress completion inside a string or comment, using the rowan token at
    the cursor offset
  - otherwise collect the identifier prefix immediately before the cursor
- Candidate sources, in rough priority order:
  - user `module` and `function` declarations in the current file
  - top-level assignments and parameters visible at the cursor
  - built-in modules and functions
  - `$fn`, `$fa`, `$fs`, `$t`, `$children`
  - inside an `include`/`use` string, files from the workspace and configured
    library paths (optional for v1.1)
  - snippets later; do not block v1 on them
- Keep candidate collection as a pure function over `(source, cursor_byte)` so
  it can be unit-tested without egui.
- Use `egui::CompletionPopup` (new in egui 0.37) around the `TextEdit` inside
  `CodeEditor`. Its `show` closure receives
  `CompletionQuery { text, word, word_range }`; return `Suggestion`s from the
  pure candidate function. The popup owns Up/Down/Tab/Enter/Escape, click
  handling and focus, so do not hand-roll an `egui::Area` or key interception.
  Verify the exact API against the 0.37 source after Phase 0.
- Configure `CompletionPopup::word_boundary` for OpenSCAD identifiers (including
  `$` and digit-leading names); the default word rules may be too narrow.
- `CompletionPopup` already deletes `word_range` and inserts `accepted.insert`
  into the `TextEdit` buffer, then restores the cursor. Do **not** apply it
  again through `apply_edit`; that would double-insert. Use
  `CompletionOutput.accepted` only to mark the document dirty, refresh the
  parse/diagnostic caches, and notify the LSP.
- The popup mutates the buffer after the `TextEdit` has run, so do not rely
  only on `TextEditOutput.response.changed()` to notice a completion; check
  `accepted.is_some()` as well. Verify that the insertion becomes exactly one
  undo step; add an explicit transaction boundary if the popup's edit is not
  captured by the undoer.
- If the cursor is in an unsupported context, return an empty suggestion list
  rather than a misleading popup.

### Editing behavior

Keep v1 deliberately small:

- Give the `TextEdit` and the completion popup one stable `Id` (for example
  `ui.make_persistent_id("source_editor")`). `TextEditState`, undo and
  `CompletionPopup` all store state under that id, so an auto-generated id
  would lose them across layout changes.

- Ctrl+Enter evaluates, as today.
- Ctrl+S saves, as today.
- Tab inserts the configured indent when there is no selection; Shift+Tab
  dedents selected lines. Use `apply_edit` per line in descending order.
- Auto-indent on Enter is a stretch item for Phase 1, not a blocker. If added,
  compute the current line's leading whitespace from the CST or source text.
- Bracket matching is optional in v1; a simple highlight of the matching
  `()[]{}` is enough if time allows.

### Left panel layout

The current left panel is one `ScrollArea` containing the source editor, the
error label, the shape tree and the properties form. A long source file pushes
an error or the properties out of reach, and the sections cannot be sized
independently.

Replace it with a vertical stack of resizable sections inside the existing
`SidePanel::left("source")`:

- Source editor (the `CodeEditor` above), with its own vertical scroll.
- Error/diagnostics strip, with its own scroll. Keep it compact and visible;
  do not put it after the shape tree.
- Shape tree, with its own vertical scroll.
- Properties form, with its own vertical scroll.

Each section gets a draggable horizontal divider. Prefer nested
`TopBottomPanel::show_inside` (resizable) or a small splitter helper over a
single shared `ScrollArea`; the panel sizes live in egui memory (session-only
for now). The 3D viewport stays in the central panel. Gutter, current-line and
diagnostic painting stay inside the editor's own scroll area.

Acceptance: each section scrolls independently; dragging a divider changes the
space without moving the others' scroll offsets; an error is visible without
scrolling past the source; window resize and the headless screenshot still lay
out sanely.

### Phase 1 acceptance

- The source pane shows rowan-based highlighting, line numbers, the current
  line, and parse-error markers.
- Typing, selection, copy/paste, IME input, and Ctrl+Enter still work.
- Completion lists user declarations and built-ins, and inserts the selected
  item at the cursor.
- Unit tests cover highlighting sections (including that concatenated sections
  equal the source), identifier classification, diagnostic range conversion,
  completion context detection, and indentation arithmetic.
- The headless screenshot shows the new editor without layout regressions.
- Source, diagnostics, shapes and properties each have their own scroll area
  and resizable divider.

## Phase 2 --- application menu bar and edit commands

The current top panel has two rows: a heading/Evaluate row and a row with
permanent `Open…`, `Save`, and `Save As…` buttons. Replace the second row with a
conventional menu bar and keep only the high-frequency actions in the toolbar.

The viewer now renders on demand: the offscreen 3D texture is reused while the
camera, size and scene are unchanged. Editor and menu actions that change the
3D scene must invalidate that gate; source-only edits do not. Do not reintroduce
an unconditional `ViewportCommand` or `request_repaint` in the update loop.

### Menu layout

Use `egui::MenuBar` (from `egui::containers::menu`, or `egui::menu::bar` before
the Phase 0 bump) in the top panel.

- **File**: New, Open…, Save, Save As…, Reload from Disk, Quit
- **Edit**: Undo, Redo, Cut, Copy, Paste, Paste as Plain Text, Select All,
  Toggle Line Comment, Indent, Outdent
- **View**: Fit View, Reset Camera, Show/Hide Source, Show/Hide Shapes
  (the last two are optional)
- **Help**: Keyboard Shortcuts, About BrepSCAD

Deferred edit commands: Duplicate Line, Move Line Up/Down, Go to Line,
Find/Replace (Phase 6), and a command palette.

Keep `Evaluate` as the primary action in the toolbar row, along with the legacy
centering checkbox and the analytic preview checkbox, evaluation spinner,
elapsed time, current path, and modified marker. Remove the permanent `Open…`,
`Save`, and `Save As…` buttons; those move under File.

The two preview toggles are only in the toolbar for now. When the menu bar
lands, both move into a **View** menu ("Legacy OpenSCAD centering" and
"Analytic preview"), each as a checkable item with its hover text as the
description; the toolbar keeps `Evaluate` and the status readouts.

Use `ui.add_enabled(...)` for unavailable actions. Show the shortcut text next
to menu items using `Modifiers::COMMAND` and platform-appropriate labels ("Ctrl"
vs "Cmd"). The menu bar is in-window on every platform; a native macOS menu bar
is out of scope for now.

### One command path

Add an editor action enum and one execution function:

```rust
enum EditAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    PastePlain,
    SelectAll,
    ToggleLineComment,
    Indent,
    Outdent,
}
```

Both menu items and keyboard shortcuts must call `CodeEditor::action(...)`.
Do not duplicate edit logic between the two. The action returns whether it
changed the source; the app then updates `dirty`, diagnostics, and the
parameter list exactly as it does for typing.

- **Undo/Redo.** `TextEditState::undoer()` returns a clone of the
  `TextEditUndoer`, which is `Undoer<(CCursorRange, String)>`.
  `has_undo`, `has_redo`, `undo` and `redo` all take the current
  `&(CCursorRange, String)` state. Replace the source and cursor from the
  returned state, then store the modified undoer back with
  `TextEditState::set_undoer` and store the state. `TextEdit` feeds the undoer
  each frame, so parameter-panel edits should become undoable; note that the
  undoer is time-based, so a fresh edit becomes an undo point after the stable
  interval. Verify grouping and add an explicit transaction boundary only if
  needed.
- **Select All.** Use `CCursorRange::select_all_str` through the stored
  `TextEditState`.
- **Cut/Copy.** Read the selected char range from the editor state, convert to
  bytes, and use `ctx.copy_text(...)`. Cut also deletes the selection through
  the shared edit path.
- **Paste.** Read text with `arboard`, which is already in the dependency tree
  through eframe's clipboard feature. Normalize CRLF/CR to LF, insert at the
  selection, and restore the cursor. Add `arboard` as a direct dependency
  rather than adding a separate clipboard implementation.
- **Paste as plain text.** egui exposes only text clipboard data, so v1 uses the
  same insertion path after stripping NULs and normalizing line endings. Keep
  the menu item because it is conventional and because a future rich clipboard
  could diverge from plain text.
- **Toggle Line Comment.** Operate on selected lines, or the current line when
  there is no selection. Add `// ` after the existing indentation unless every
  non-blank selected line is already commented, in which case remove the
  comment prefix. Apply edits from the bottom up with `apply_edit` and restore
  the selection.
- **Indent/Outdent.** Share the Tab/Shift+Tab implementation from Phase 1.

If `arboard` is rejected, the fallback is to make the Paste menu item focus the
editor and explain that the platform paste shortcut should be used. That is a
worse UX, so prefer the direct dependency.

### File actions and dirty state

- `New` starts an empty `.scad` buffer with no path.
- `Open…`, `New`, `Reload from Disk`, and `Quit` check `dirty` and show a
  Save / Discard / Cancel modal before replacing or closing the buffer.
- `Save` writes the current path or falls back to Save As.
- `Save As…` uses the existing `rfd` dialog.
- `Reload from Disk` is disabled when the buffer has no path.
- Window-close requests go through the same dirty check; use
  `ViewportCommand::CancelClose` when the user cancels.

### Keyboard shortcuts

Keep the existing Ctrl+Enter evaluate and Ctrl+S save bindings. Add the standard
editor bindings and route them through the same command path:

- `Command+Z`: Undo; `Command+Shift+Z` and `Command+Y`: Redo
- `Command+X/C/V`: Cut/Copy/Paste
- `Command+Shift+V`: Paste as plain text
- `Command+A`: Select All
- `Command+/`: Toggle line comment (accept the Shift variant produced by some
  keyboard layouts)
- Tab/Shift+Tab: Indent/outdent when the editor has focus

Use `Modifiers::COMMAND` so macOS gets Command and other platforms get Ctrl. Do
not intercept a shortcut while a different text widget, such as a parameter
`DragValue`, has focus unless the action explicitly targets the editor.

### Tests and acceptance

- Unit-test the pure helpers: comment toggling (including indentation and
  already-commented lines), line-ending normalization, selection-to-byte
  conversion, and menu enablement predicates.
- Test undo/redo through the editor state for typing, a completion insertion,
  and a parameter-panel edit.
- Verify the menu bar and each menu action with the headless screenshot flow and
  a manual pass.
- Acceptance: File/Edit/View/Help are present; Open/Save/Save As are no longer
  permanent buttons; Undo/Redo/Cut/Copy/Paste/Paste as Plain Text/Select All/
  Toggle Comment work from both the menu and the keyboard; destructive file
  actions prompt when dirty.

## Phase 3 --- structured diagnostics

This phase exists because the LSP server should publish ranges, not strings.

### Diagnostic type

Add a small viewer/LSP-facing diagnostic type. `SyntaxError` and `EvalError`
keep their own types and gain ranges; the editor and server map those into this
type rather than making the syntax/eval crates depend on a UI type. Prefer that
mapping over a new crate:

```rust
pub enum Severity { Error, Warning, Info }

pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub range: Range<usize>,       // byte offsets in the file
    pub source_path: Option<PathBuf>,
}
```

- `brepscad-syntax` produces parse diagnostics. `SyntaxError` already stores an
  offset; convert it to a range at the next token or one character.
- `brepscad-eval` produces evaluator diagnostics.

### Evaluator spans

`EvalError` is currently mostly string-only (`BadArguments` also carries a
module name, and `Kernel` wraps a `KernelError`) with about 75 construction
sites. The preferred refactor is:

1. Split the existing variants into `EvalErrorKind`.
2. Make `EvalError` a struct with `kind: EvalErrorKind` and
   `span: Option<TextRange>`.
3. Thread the current syntax node's range into errors at module calls,
   expression evaluation, argument binding, and include/use handling first.
4. Leave unimportant internal errors span-less rather than blocking the
   refactor.
5. Keep `Display` output unchanged so CLI messages do not churn.
6. Keep `impl From<KernelError> for EvalError` working so `?` at the kernel
   boundary does not churn.

Do this as a mechanical, test-backed refactor. Do not combine it with a parser
change or a kernel change.

### Included files

- Add the source path to diagnostics for included files.
- Map included-file ranges to their own files in the editor and server.
- This is already listed as a limitation in `status.md`; it becomes more
  visible once diagnostics are interactive.

### Phase 3 acceptance

- Parse errors have ranges and underline the correct token.
- Representative evaluator errors (unknown function, bad arguments, assertion,
  source/include failure) carry a useful range.
- Diagnostics from included files name the included file.
- CLI output and the corpus parse/evaluate baselines are unchanged except for
  intentional diagnostic-message improvements.

## Phase 4 --- `brepscad-lsp`

### Crate and dependencies

Add `crates/scad-lsp` with package name `brepscad-lsp` and a `brepscad-lsp`
binary. It is a separate binary, so it can depend on the evaluator without
making the viewer heavier.

Proposed dependencies:

- `async-lsp` 0.2.4 (client and server framework over tower; it pins
  `lsp-types` 0.95)
- `lsp-types` 0.95.0 (match `async-lsp`; do not add 0.97 alongside it). Prefer
  `async_lsp`'s re-export if it has one; otherwise pin `=0.95.0` exactly.
- start without `lsp-textdocument`: 0.5.0 depends on `lsp-types` 0.97, so it
  would create a second, incompatible LSP type universe. A small in-house
  `documents.rs` (`Uri -> version, text`) is enough while sync is FULL.
  If incremental sync needs a library, find a version that resolves to
  `lsp-types` 0.95 and check with `cargo tree -d`.
- `tokio` 1 with `rt-multi-thread`, `io-std`, `io-util`, `macros`, `sync`, and
  `time`; add `process` for the Phase 5 child-process client
- `serde`, `serde_json`, `anyhow`, `thiserror`, `url`
- `brepscad-syntax` and `brepscad-eval` (for built-ins and include resolution)

`tokio` is a new workspace dependency. Keep it out of the viewer until Phase 5,
and consider a minimal thread-based transport if the runtime cost or dependency
surface is unwanted.

### Server shape

```text
crates/scad-lsp/src/
  main.rs         stdio bootstrap and capability registration
  server.rs       async-lsp handlers and server state
  documents.rs    URI -> (version, text); full sync first; in-house store
  positions.rs    UTF-16 Position <-> byte offset
  analysis.rs     declarations, scopes, include resolution
  diagnostics.rs  parse and static checks
  completion.rs   candidates and completion items
  hover.rs        built-in and declaration documentation
  definition.rs   identifier -> declaration/file
  symbols.rs      document symbols
```

### Protocol decisions

- Use push diagnostics (`textDocument/publishDiagnostics`) in v1. Pull
  diagnostics can come later if a client needs them.
- Start with `TextDocumentSyncKind::FULL`. Move to incremental sync once the
  document store is stable; `apply_edit` already provides the right primitive.
- LSP positions default to UTF-16 code units. Implement and test
  `Position <-> byte offset` conversion for non-ASCII text before any handler
  relies on it.
- Do static analysis only. Do not run geometry evaluation on every change.
  Evaluator diagnostics can be added later as an explicit command or on save,
  with a timeout.
- Resolve `include`/`use` paths with the same rules as the evaluator: quoted
  paths relative to the including file, angle-bracket paths through
  `OPENSCADPATH` and configured library paths.

### Features for v1

- `initialize`, `initialized`, `shutdown`, `exit`
- `textDocument/didOpen`, `didChange`, `didSave`, `didClose`
- diagnostics:
  - parse errors (always)
  - duplicate top-level declaration names (warning)
  - unknown module/function call (warning, based on built-ins plus visible
    declarations and included files)
  - do not flag unknown variables or arity in v1; BOSL2's dynamic patterns
    would produce too many false positives
- completion: built-ins, user modules/functions, top-level assignments, visible
  parameters, and `$` special variables
- hover: built-in signatures/docs, user declaration signatures and defaults
- definition: identifiers to declarations; `include`/`use` string paths to
  files
- document symbols: modules, functions, and top-level assignments

Deferred to later: formatting, code actions, rename, references, semantic
tokens, signature help, inlay hints, and pull diagnostics.

### Built-in metadata

Add a table in `brepscad-eval` (for example `src/builtins.rs`) with name, kind
(module or function), parameter signature, and short documentation. The
evaluator can adopt it incrementally; the LSP uses it immediately for
completion and hover. Do not duplicate a second hand-maintained list in the LSP
crate.

### Testing

- Unit tests for UTF-16 position conversion, including non-ASCII and CRLF.
- Handler tests against an in-memory document store for diagnostics,
  completion, hover, and definition.
- An integration test that starts the binary, sends `initialize`, `didOpen`,
  and a request over stdio, and asserts the JSON response.
- Include a fixture with a quoted include and an angle-bracket include.
- Run `cargo test -p brepscad-lsp` in the full check.

### Phase 4 acceptance

- `brepscad-lsp` runs over stdio and works with at least one external client
  (Helix, Neovim, or VS Code) on `assets/sample.scad`.
- Parse diagnostics appear in the client with correct ranges.
- Completion, hover, and go-to-definition work for built-ins and user
  declarations.
- The server does not depend on a GPU or a kernel backend.
- Ship `brepscad-lsp` beside the viewer in release packaging and document how
  to point external clients at it.

## Phase 5 --- in-app LSP client

- Spawn `brepscad-lsp` as a child process from the viewer, or connect to a
  configured server command. Look next to the viewer binary, then `PATH`, then
  a `BREPSCAD_LSP` override.
- Use `async-lsp`'s client role on a background runtime thread, with channels
  to and from the egui thread. Keep the UI thread non-blocking.
- Track the open document URI and version. Send `didOpen` when a file is
  opened, debounced `didChange` on edits (roughly 150--250 ms), `didSave` on
  save, and `didClose` on close.
- Map LSP diagnostics into the same `Diagnostic` type the editor already uses.
  Only current-file diagnostics are required in v1; included-file diagnostics
  can be shown in a problems list later.
- Reuse the completion popup for LSP items. Local completion remains the
  fallback when the server is absent.
- Add hover through a small popup or a status-bar area, and go-to-definition
  via F12 or Ctrl+Click. Restore the cursor through `TextEditState`.
- Ignore stale responses by request id or document version.
- If the server fails to start or exits, log the reason once and keep the
  local editor fully functional. Add a restart command.

### Phase 5 acceptance

- With the server available, diagnostics, completion, hover, and definition
  work in the app.
- With the server removed, the editor still works and local parse diagnostics
  and completion still appear.
- No UI thread stalls while the server starts, responds, or restarts.

## Phase 6 --- polish and deferred items

Schedule these as separate small changes after the main path works:

- **Undo/redo grouping and transactions.** Phase 2 exposes Undo/Redo and routes
  menu and keyboard edits through one command path. Refine grouping across
  parameter drags and completion insertions, and add explicit transaction
  boundaries only if the `TextEdit` undoer produces noisy steps.
- **Recent files.** Populate the File menu once recent-file handling exists.
- **Find and replace.** `TextEdit` does not provide it. Add a search bar,
  highlight matches through the layouter, and apply replacements through the
  edit path.
- **File watching and external changes.** Add `notify` to the viewer, prompt on
  external modification, and refresh included files.
- **Included-file tabs or diagnostics.** Map included-file ranges back to their
  own files, as already noted in `status.md`.
- **Formatting.** Add `textDocument/formatting` in the LSP using the CST.
  Start with indentation normalization; do not reformat aggressively.
- **Snippets and signature help.** Add after completion and hover are stable.
- **Code actions.** Start with quick fixes for unknown built-ins and duplicate
  declarations.
- **Semantic tokens.** Optional; rowan highlighting is already sufficient for
  the in-app editor.
- **Performance.** Keep the parse cache, avoid reparsing in the layouter, and
  debounce LSP requests for large library files.
- **Accessibility and keyboard coverage.** Verify Ctrl/Cmd shortcuts, IME,
  screen-reader behavior, and focus traversal.
- **Documentation.** Add LSP client setup examples to `development.md` and
  update `status.md` and the README documentation list once the plan lands.

## Future --- full Floem switchover

This section is a sketch, not scheduled work. Revisit only if multi-cursor,
modal editing, folding, or a Lapce-class editor becomes a hard requirement.

### What would change

- Replace `eframe`/`egui` as the application shell with Floem. Floem owns the
  window, event loop, and its own forked `winit`; it cannot be embedded inside
  an eframe app.
- Move the 3D viewport from the current `egui_wgpu` offscreen-texture path into
  a Floem view/renderer integration. This is the go/no-go item.
- Replace the custom egui editor with `floem::views::editor` backed by
  `floem-editor-core`. Keep `brepscad-syntax` and implement Floem's
  `Document`/`Styling` traits for highlighting and phantom text.
- Keep the LSP server and the diagnostic/completion logic. Rebuild the popup,
  hover, and go-to-definition presentation as Floem overlays.
- Rebuild the side panels, resizable splits, tree, parameter forms, menus,
  file dialogs, and status bar.

### Version and maintenance cost

- Published `floem` 0.2.0 is from November 2024 and uses `wgpu` 22 with
  `winit` 0.29.5.
- Floem main requires Rust 1.91 and currently uses `wgpu` 27 and `parley` 0.7.
- Lapce pins a Floem git revision and, as of this writing, requires Rust 1.98.
- A switchover means tracking a pre-1.0 framework at a pinned git revision,
  not consuming a stable crates.io release.

### Work items and decision gate

1. **Timeboxed spike:** open a Floem window, render the existing `MeshRenderer`
   into it, and handle orbit/pan/zoom. If this cannot be made clean, stop.
2. Port the shell and panels.
3. Port the viewport, picking, and camera.
4. Adopt the Floem editor and implement `Document`/`Styling`.
5. Port the LSP client and build completion/hover/definition overlays.
6. Port theming, accessibility, packaging, and the headless screenshot flow.

Keep if the switch happens: `brepscad-syntax`, `brepscad-eval`, the kernel
crates, the CLI, the LSP server, diagnostics, and completion candidate logic.
The current wgpu renderer is reusable only if the Floem/pinned-wgpu versions
align; expect a port.

Risks: pre-1.0 breakage, `winit` fork constraints, renderer interop, a large
editor-widget API surface (about 11k lines of Floem editor code), no folding,
and a much smaller testing story for the UI.

Floem and `floem-editor-core` are MIT; Lapce is Apache-2.0. Both are compatible
with this project's BSD-3-Clause licence, but keep copyright notices if code is
copied rather than depended on.

## Open decisions

- **Menu bar scope:** in-window menu bar on every platform for v1. Native macOS
  menu integration is deferred.
- **Clipboard dependency:** add `arboard` directly for menu Paste/Paste as
  plain text. It is already transitive through eframe's clipboard feature, so
  this should not add a new crate.
- **Undo model:** use egui's `TextEditUndoer` first. Add an application-level
  undo stack only if parameter-panel edits group poorly or multiple edit
  sources need transactions.
- **Tokio in the viewer:** needed for `async-lsp` if the client shares the
  server framework. Alternative: a minimal JSON-RPC client over threads and
  channels. Decide before Phase 5.
- **Static-only LSP vs evaluator diagnostics:** this plan assumes static-only
  v1 for latency. Confirm before Phase 4.
- **Editor crate boundary:** keep the editor in `crates/viewer` initially.
  Extract only if it must be reused.
- **Rowan 0.17:** bump in Phase 0 if the workspace tests pass. Do not move to
  0.18 alphas.
- **Incremental sync:** full sync first. Decide after the document store and
  position conversion are tested.
- **LSP dependency triad:** `async-lsp` 0.2.4 pins `lsp-types` 0.95, while
  `lsp-textdocument` 0.5.0 pins 0.97. Do not add both. Prefer the in-house
  document store in v1, or prove a compatible trio with `cargo tree -d` first.
- **Phase 0 landing:** land the dependency bump as its own commit/branch before
  the editor work, then rebase `feature/editor-lsp`. Keep it out of editor
  commits so it can merge to `main` independently.
- **Formatting scope:** indentation only, or a full CST-based formatter?
  Deferred until after the LSP v1.

## Verification commands

Use `nix-shell --run '...'` around these when working outside the dev shell.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cargo run -p brepscad -- assets/sample.scad
cargo run -p brepscad-viewer
scripts/screenshot-viewer.sh /tmp/viewer.png

scripts/check-scad-corpus.sh --parse-only --baseline corpus/baseline-parse.tsv .corpus

cargo test -p brepscad-lsp
```

## Related docs

- [`status.md`](status.md) --- current capabilities, limitations, and next steps.
- [`development.md`](development.md) --- development helpers and headless UI
  snapshots.
- [`research.md`](research.md) --- prior art and architecture decisions.
- [`kernels.md`](kernels.md) --- kernel capability matrix.
