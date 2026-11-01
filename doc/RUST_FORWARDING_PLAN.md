# Less boilerplate for calls into the "base classes": plan

Status: 2026-11-01, A and B done. 535 -> 292 `self.base.` / `self.inner.` calls in `src/`
(about 780 lines less, the new macros included). What is left is not a pure forward: calls with hooks
(`on_grid_note(&mut SequencerHooks, ...)`), button dispatch, functions which do more than
forwarding, and forwards to a function with another name (`on_button` -> `seq.on_button` without
the velocity). The sequencer base got the `View` variants `on_sequencer_left`/`right` and
`(set_)sequencer_resolution_index`, so that the views forward them. The blanket "has a base"
trait (see Decision) was not needed so far.

## Problem

The Java class hierarchy (`AbstractSequencerView` → `AbstractNoteSequencerView` →
`MaschineJamSequencerView`, `AbstractTrackMode` → `TrackVolumeMode` → `MaschineJamVolumeMode`,
…) is ported as composition: a controller type holds the framework type in a field `base` or
`inner` and forwards each trait method by hand. There are about 570 `self.base.` / `self.inner.`
calls in `src/`, most of them one-line forwards. The worst files:

| File | Calls |
|---|---|
| `controller/maschine/jam/view/sequencer_views.rs` | 73 |
| `controller/maschine/jam/view/drum_views.rs` | 58 |
| `framework/mode/device.rs` | 49 |
| `controller/maschine/mk/view/drum_view.rs` | 32 |
| `controller/launchpad/view/drum_view.rs` | 29 |
| `controller/launchpad/view/{poly_sequencer,drum_lane,sequencer,raindrops}_view.rs` | 16-22 each |

What gets repeated:

1. **`View` (33 functions) and `Mode` (27 functions) forwards**: `name`, `on_activate`,
   `on_deactivate`, `button_color_id`, `draw_grid`, `update_control_surface`,
   `update_note_mapping`, `on_sequencer_left`/`right`, `sequencer_resolution_index`, the knob and
   item functions of `Mode`, ...
2. **`TransposeView` forwards**: the five functions, implemented by forwarding about 27 times.
3. **The facet getters on `View`**: `as_transpose_view(_mut)` (19 impls),
   `session_alternative(_mut)` (16), `as_virtual_fader_view` (6). Each is a pair of functions
   returning `Some(self)`. The defaults in `framework/feature.rs` already return `None`; only the
   overrides are boilerplate.

Already there: `impl_as_any!` (`framework/feature.rs`, 79 uses); the module-local
`forward_mode!`, `forward_pages!`, `forward_touch!` (`controller/maschine/jam/mode/mod.rs`) and
`forward_track_mode!` (`controller/maschine/mk/mode/track_modes.rs`), 18 uses. These hardcode the
field `inner` and forward a fixed set of functions, so a type which overrides one of them cannot
use the macro and writes the whole set by hand.

## Decision

Two dependency-free steps first (the crate has no dependencies, and this stays so):

- **A. Selective forwarding macros** in the framework, replacing the local ones.
- **B. One-line macros for the facet getters**, like `impl_as_any!`.

Not now (maybe later): a blanket "has a base" trait (`trait SequencerViewImpl { fn base(&self)
-> &NoteSequencerBase; ... }` + `impl<T: SequencerViewImpl> TransposeView for T`) for the
sequencer and drum view families; the `delegate` / `ambassador` crates.

## A. Forwarding macros

### Shape

```rust
impl View for SequencerView {
    // AbstractSequencerView
    crate::forward_view!(base.seq => name, button_color_id, update_control_surface,
        on_sequencer_left, on_sequencer_right);
    // AbstractNoteSequencerView
    crate::forward_view!(base => on_activate, draw_grid, update_note_mapping);

    fn on_grid_note(&mut self, ctx: &mut Ctx, note: i32, velocity: i32) {
        self.base.on_grid_note(&mut SequencerHooks, ctx, note, velocity);
    }

    // ... the functions which do something of their own stay hand-written

    impl_as_any!();
}
```

- The target is a field path relative to `self` (`inner`, `base`, `base.seq`). Several
  invocations with different targets are allowed in one `impl`.
- Only the listed functions are forwarded. Overriding a function means leaving it out of the
  list and writing it, no exclude lists.
- The macro knows the signature of each function by name: one internal arm per function of the
  trait, the public arm loops over the list.

  ```rust
  #[macro_export]
  macro_rules! forward_view {
      ($($field:ident).+ => $($fun:ident),+ $(,)?) => {
          $( $crate::forward_view!(@fun $fun, $($field).+); )+
      };
      (@fun name, $($field:ident).+) => {
          fn name(&self) -> &str {
              self.$($field).+.name()
          }
      };
      (@fun on_activate, $($field:ident).+) => {
          fn on_activate(&mut self, ctx: &mut $crate::framework::ctx::Ctx) {
              self.$($field).+.on_activate(ctx);
          }
      };
      // ... one arm per View function
  }
  ```

  An unknown function name is a compile error ("no rules expected the token"), which is what we
  want. Use full paths (`$crate::...`) for the parameter types so that the call site needs no
  extra `use`.
- The target only needs a method with the same name and signature, whether inherent (the
  `*Base` structs) or from the trait (a wrapped `Mode`).

### Macros

| Macro | Trait | Notes |
|---|---|---|
| `forward_view!` | `View` | all 33 functions except `as_any(_mut)` and the facet getters (B) |
| `forward_mode!` | `Mode` | all 27 functions except `as_any(_mut)` |
| `forward_transpose_view!` | `TransposeView` | the five functions |

Put them into a new module `framework/forward.rs` (`#[macro_export]`, so they are used as
`crate::forward_view!`), with a short module doc pointing to this file's shape section.

### Signature mismatches to fix first

The macros only work where the base has the trait's signature. Known case:
`is_octave_up_button_on` / `is_octave_down_button_on` take `ctx: &Ctx` in the trait,
`framework/view/play_view.rs` and `framework/view/sequencer/abstract_drum_view.rs`, but no `ctx`
in `abstract_note_sequencer_view.rs`, `abstract_poly_sequencer_view.rs`,
`abstract_raindrops_view.rs` and `drum64_view.rs`. Give all of them the `ctx` parameter (as the
Java `TransposeView` methods are the same on all). Other mismatches turn up as compile errors
during the migration; fix them in the base the same way, or keep that function hand-written.

### Migration order

1. `framework/forward.rs` with the three macros; a test module with a dummy `View`/`Mode`
   which uses every arm once, so each arm is compiled even before all users are migrated.
2. Replace the local macros: `forward_mode!`, `forward_pages!`, `forward_touch!` in
   `controller/maschine/jam/mode/mod.rs` by `crate::forward_mode!(inner => ...)`.
   `forward_track_mode!` in `controller/maschine/mk/mode/track_modes.rs` does more than
   forwarding (the index mapping by `track_index`, the touch state): keep it, but build the
   pure forwards in it on `crate::forward_mode!`.
3. The views, worst first: `jam/view/sequencer_views.rs`, `jam/view/drum_views.rs`,
   `mk/view/drum_view.rs`, the launchpad views.
4. The framework modes: `framework/mode/device.rs`, `master.rs`, `layer.rs`, then the controller
   modes (`launchcontrol/mode/*`, `mk/mode/*`).

One commit per step (step 3 and 4 may be one commit per controller).

### Comments

The rule "comments name the Java class and method" stays: put one comment above each
`forward_*!` invocation naming the Java class whose methods it stands for (see the shape above).
Hand-written functions keep their per-function comments.

## B. Facet getter macros

```rust
impl View for SequencerView {
    // ...
    crate::impl_transpose_view!();
    crate::impl_session_alternative!();
    impl_as_any!();
}
```

| Macro | Expands to |
|---|---|
| `impl_transpose_view!` | `as_transpose_view` + `as_transpose_view_mut` returning `Some(self)` |
| `impl_session_alternative!` | `session_alternative` + `session_alternative_mut` returning `Some(self)` |
| `impl_virtual_fader_view!` | `as_virtual_fader_view` returning `Some(self)` |

Next to `impl_as_any!` in `framework/feature.rs`, same style. `note_editor(_mut)` has only 2
overrides which return a field, not `self`: leave them.

B is independent of A and small; do it first (one commit), then A.

## Verification

This is a pure refactoring: no behaviour changes.

- `cargo build` without new warnings, `cargo test` all green, including the replays under
  `tests/` (no replay may need to be recorded again; if one does, something was forwarded to
  the wrong target).
- Per step, check that the number of `self.base.` / `self.inner.` calls in the touched files went
  down and that no hand-written forward remains for a function the macro could take.
- A conformance run on Bitwig is not needed for the refactoring itself; run one for a
  controller if the signature fixes (`is_octave_*_button_on`) changed more than the parameter
  list.
