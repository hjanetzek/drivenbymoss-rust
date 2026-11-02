// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Several control surfaces in one controller instance (former `AbstractControllerSetup.surfaces`
//! with `getSurface (index)`), e.g. a Mackie MCU with its extenders. See
//! `doc/MULTI_SURFACE_PLAN.md`.
//!
//! The context keeps one *current* surface in its fields ([`Ctx::surface`], [`Ctx::views`],
//! [`Ctx::modes`], [`Ctx::extra_modes`], [`Ctx::text_display`], [`Ctx::ext`], the MIDI and
//! sysex handlers). The parts of the other surfaces are parked in a [`SurfaceSet`];
//! [`Ctx::select_surface`] swaps them in. All existing code therefore works on "the" surface
//! unchanged, and a controller with one surface never swaps anything.
//!
//! - The app selects the surface whose MIDI input received a message before it is handled, and
//!   selects the first surface again afterwards (see [`Ctx::with_surface`]).
//! - A task scheduled with [`Ctx::schedule`] runs with the surface which was current when it
//!   was scheduled (e.g. the long press check of a button).
//! - The model, the configuration, the host, the colors and the scheduler are shared.
//!
//! Java features:
//! - `MCUControlSurface.isPressed` checks the buttons of all surfaces (Shift is only on the
//!   main device): [`Ctx::share_button_states`].
//! - `FeatureGroupManager.addConnectedManagerListener` keeps the mode managers of all surfaces
//!   in sync: [`Ctx::connect_surface_mode_managers`].

use std::any::Any;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Instant;

use super::ctx::{Ctx, MidiHandler, SysexHandler};
use super::display::TextDisplay;
use super::feature::{FeatureGroupManager, ModeManager, ViewManager};
use super::ids::{ButtonId, Modes};
use super::surface::Surface;

/// The parts of a control surface which are swapped in and out of the context.
pub struct SurfaceParts {
    pub surface: Surface,
    pub views: ViewManager,
    pub modes: ModeManager,
    pub extra_modes: Vec<ModeManager>,
    pub midi_handler: Option<MidiHandler>,
    pub sysex_handler: Option<SysexHandler>,
    pub text_display: Option<TextDisplay>,
    pub ext: Box<dyn Any>,
    notification_end: Option<Instant>,
}

impl SurfaceParts {
    /// The parts of a new surface: no views, modes, handlers or display yet.
    pub fn new(surface: Surface) -> Self {
        SurfaceParts {
            surface,
            views: FeatureGroupManager::default(),
            modes: FeatureGroupManager::default(),
            extra_modes: Vec::new(),
            midi_handler: None,
            sysex_handler: None,
            text_display: None,
            ext: Box::new(()),
            notification_end: None,
        }
    }

    /// Exchange these parts with the ones in the context.
    fn swap_with(&mut self, ctx: &mut Ctx) {
        std::mem::swap(&mut self.surface, &mut ctx.surface);
        std::mem::swap(&mut self.views, &mut ctx.views);
        std::mem::swap(&mut self.modes, &mut ctx.modes);
        std::mem::swap(&mut self.extra_modes, &mut ctx.extra_modes);
        std::mem::swap(&mut self.midi_handler, &mut ctx.midi_handler);
        std::mem::swap(&mut self.sysex_handler, &mut ctx.sysex_handler);
        std::mem::swap(&mut self.text_display, &mut ctx.text_display);
        std::mem::swap(&mut self.ext, &mut ctx.ext);
        std::mem::swap(&mut self.notification_end, &mut ctx.notification_end);
    }
}

/// The parked surfaces of a context (stored in [`Ctx::surfaces`]).
#[derive(Default)]
pub struct SurfaceSet {
    /// The parts of all surfaces, `None` at the index of the current surface (which is in the
    /// context). Empty as long as there is only one surface.
    parked: Vec<Option<SurfaceParts>>,
    current: usize,
    share_button_states: bool,
    is_syncing_modes: bool,
}

impl SurfaceSet {
    /// The index of the surface to restore for a scheduled task; `None` with only one surface.
    pub(crate) fn task_surface(&self) -> Option<usize> {
        (!self.parked.is_empty()).then_some(self.current)
    }
}

impl Ctx {
    /// Add another surface (an extender) and return its index. The surface of the context is
    /// the surface with index 0.
    pub fn add_surface(&mut self, surface: Surface) -> usize {
        if self.surfaces.parked.is_empty() {
            self.surfaces.parked.push(None);
        }
        self.surfaces.parked.push(Some(SurfaceParts::new(surface)));
        self.surfaces.parked.len() - 1
    }

    /// The number of surfaces (at least 1).
    pub fn surface_count(&self) -> usize {
        self.surfaces.parked.len().max(1)
    }

    /// The index of the current surface (former `surface.getSurfaceID ()`).
    pub fn surface_index(&self) -> usize {
        self.surfaces.current
    }

    /// Make the surface with the given index the current one. Unknown indices are ignored.
    pub fn select_surface(&mut self, index: usize) {
        if index == self.surfaces.current || index >= self.surfaces.parked.len() {
            return;
        }
        let Some(mut parts) = self.surfaces.parked[index].take() else {
            return;
        };
        parts.swap_with(self);
        let previous = self.surfaces.current;
        self.surfaces.parked[previous] = Some(parts);
        self.surfaces.current = index;
        if self.surfaces.share_button_states {
            let pressed: BTreeSet<ButtonId> = self
                .surfaces
                .parked
                .iter()
                .flatten()
                .flat_map(|parts| parts.surface.buttons.values().filter(|b| b.is_pressed()).map(|b| b.id))
                .collect();
            self.surface.pressed_on_other_surfaces = pressed;
        }
    }

    /// Run the function with the surface of the given index as the current surface; the
    /// previous surface is current again afterwards.
    pub fn with_surface<R>(&mut self, index: usize, f: impl FnOnce(&mut Ctx) -> R) -> R {
        let previous = self.surfaces.current;
        self.select_surface(index);
        let result = f(self);
        self.select_surface(previous);
        result
    }

    /// Run the function for each surface (former `this.surfaces.forEach`).
    pub fn for_each_surface(&mut self, mut f: impl FnMut(&mut Ctx)) {
        for index in 0..self.surface_count() {
            self.with_surface(index, &mut f);
        }
    }

    /// Access the parts of a parked surface (`None` for the current surface, whose parts are in
    /// the context, and for unknown indices).
    pub fn parked_surface(&self, index: usize) -> Option<&SurfaceParts> {
        self.surfaces.parked.get(index).and_then(|p| p.as_ref())
    }

    /// Mutable access to the parts of a parked surface, see [`Ctx::parked_surface`].
    pub fn parked_surface_mut(&mut self, index: usize) -> Option<&mut SurfaceParts> {
        self.surfaces.parked.get_mut(index).and_then(|p| p.as_mut())
    }

    /// A button counts as pressed if it is pressed on any surface (former
    /// `MCUControlSurface.isPressed` / `HUIControlSurface.isPressed`, which check "all surfaces
    /// for state button presses" like Shift).
    pub fn share_button_states(&mut self, enable: bool) {
        self.surfaces.share_button_states = enable;
        if !enable {
            self.surface.pressed_on_other_surfaces.clear();
        }
    }

    /// Keep the main mode managers of all surfaces in sync (former
    /// `addConnectedManagerListener` between all surfaces): when the mode of one surface
    /// changes, the same mode is activated on all others.
    ///
    /// Difference to Java: the siblings are always synced with `setActive`; Java calls
    /// `restore` on the siblings when a surface restores its mode (the active mode is the same,
    /// only the previous mode of the siblings can differ), and `setPreviousID` is not synced
    /// (use [`Ctx::set_previous_mode_on_all_surfaces`]).
    pub fn connect_surface_mode_managers(&mut self) {
        let listener: super::feature::ChangeListener<Modes> = Rc::new(|ctx: &mut Ctx, _old, new| {
            if ctx.surfaces.is_syncing_modes {
                return;
            }
            ctx.surfaces.is_syncing_modes = true;
            let current = ctx.surface_index();
            for index in 0..ctx.surface_count() {
                if index != current {
                    ctx.with_surface(index, |ctx| ctx.set_active_mode(new));
                }
            }
            ctx.surfaces.is_syncing_modes = false;
        });
        for index in 0..self.surface_count() {
            let listener = listener.clone();
            self.with_surface(index, move |ctx| ctx.modes.add_change_listener(listener));
        }
    }

    /// Former `ModeManager.setPreviousID` with connected managers.
    pub fn set_previous_mode_on_all_surfaces(&mut self, id: Modes) {
        self.for_each_surface(|ctx| ctx.modes.set_previous_id(Some(id)));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::backend::{BackendCall, RecordingBackend, Sender};
    use crate::config::Configuration;
    use crate::daw::model::Model;
    use crate::daw::model_setup::ModelSetup;
    use crate::framework::color::ColorManager;
    use crate::framework::command::{shared, FnTrigger};
    use crate::framework::feature::Mode;
    use crate::framework::value_changer::ValueChanger;
    use crate::host::{Host, MemoryLog};
    use crate::midi::{BindType, ButtonEvent, MidiInput, MidiOutput};
    use crate::scale::Scales;

    struct NamedMode(&'static str, Rc<RefCell<Vec<String>>>);

    impl Mode for NamedMode {
        fn name(&self) -> &str {
            self.0
        }

        fn on_activate(&mut self, _ctx: &mut Ctx) {
            self.1.borrow_mut().push(format!("+{}", self.0));
        }

        crate::impl_as_any!();
    }

    fn surface(sender: &Sender, port: i32) -> Surface {
        Surface::new(MidiOutput::new(sender.clone(), Some(port)), MidiInput::new(sender.clone(), Some(port), &[]).unwrap(), None)
    }

    fn ctx(count: i32) -> (Ctx, Rc<RecordingBackend>) {
        let backend = Rc::new(RecordingBackend::default());
        let sender = Sender::new(backend.clone());
        let host = Host::new(Rc::new(MemoryLog::default()));
        let value_changer = ValueChanger::twos_complement(128, 1);
        let scales = Scales::new(&value_changer, 36, 100, 8, 8);
        let model = Model::new(host.clone(), sender.clone(), ModelSetup::new(), value_changer, scales);
        let mut ctx = Ctx::new(host, sender.clone(), model, Configuration::new(&[]), surface(&sender, 0), ColorManager::new());
        for port in 1..count {
            ctx.add_surface(surface(&sender, port));
        }
        backend.take_calls();
        (ctx, backend)
    }

    #[test]
    fn single_surface_does_not_swap() {
        let (mut ctx, _) = ctx(1);
        assert_eq!(ctx.surface_count(), 1);
        ctx.select_surface(1);
        assert_eq!(ctx.surface_index(), 0);
        assert!(ctx.surfaces.task_surface().is_none());
        let mut count = 0;
        ctx.for_each_surface(|_| count += 1);
        assert_eq!(count, 1);
    }

    #[test]
    fn surfaces_are_swapped() {
        let (mut ctx, backend) = ctx(3);
        assert_eq!(ctx.surface_count(), 3);
        for index in 0..3 {
            ctx.with_surface(index, |ctx| {
                ctx.ext = Box::new(index);
                let id = ButtonId::get(ButtonId::F1, index);
                ctx.add_button(id, "F", Some(shared(FnTrigger(|_: &mut Ctx, _, _| {}))), BindType::Note, 0, 10, None);
            });
        }
        assert_eq!(*ctx.ext::<usize>(), 0);
        assert_eq!(ctx.with_surface(2, |ctx| *ctx.ext::<usize>()), 2);
        assert!(ctx.surface.button(ButtonId::F1).is_some());
        assert!(ctx.surface.button(ButtonId::F3).is_none());
        assert!(ctx.parked_surface(2).unwrap().surface.button(ButtonId::F3).is_some());
        assert!(ctx.parked_surface(0).is_none());

        // The lights of each surface go to its own output
        ctx.for_each_surface(|ctx| ctx.flush_surface());
        let ports: Vec<i32> = backend.take_calls().into_iter().filter_map(|c| if let BackendCall::SendMidi(port, _) = c { Some(port) } else { None }).collect();
        assert_eq!(ports, vec![0, 1, 2]);
    }

    #[test]
    fn scheduled_tasks_run_with_their_surface() {
        let (mut ctx, _) = ctx(2);
        ctx.for_each_surface(|ctx| ctx.ext = Box::new(ctx.surface_index()));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        ctx.with_surface(1, move |ctx| ctx.schedule(0, move |ctx| s.borrow_mut().push((ctx.surface_index(), *ctx.ext::<usize>()))));
        let now = ctx.host.now();
        ctx.run_due_tasks(now + std::time::Duration::from_millis(1));
        assert_eq!(*seen.borrow(), vec![(1, 1)]);
        assert_eq!(ctx.surface_index(), 0);
    }

    #[test]
    fn shared_button_states() {
        let (mut ctx, _) = ctx(2);
        ctx.with_surface(1, |ctx| ctx.add_button(ButtonId::Shift, "Shift", None, BindType::Note, 0, 70, None));
        ctx.with_surface(1, |ctx| ctx.trigger_button(ButtonId::Shift, ButtonEvent::Down, 1.0));
        assert!(!ctx.surface.is_shift_pressed());
        ctx.share_button_states(true);
        ctx.with_surface(1, |_| {});
        assert!(ctx.surface.is_shift_pressed());
        ctx.with_surface(1, |ctx| ctx.trigger_button(ButtonId::Shift, ButtonEvent::Up, 0.0));
        assert!(!ctx.surface.is_shift_pressed());
    }

    #[test]
    fn connected_mode_managers() {
        let (mut ctx, _) = ctx(3);
        let log = Rc::new(RefCell::new(Vec::new()));
        ctx.for_each_surface(|ctx| {
            ctx.register_mode(Modes::Volume, NamedMode("Volume", log.clone()));
            ctx.register_mode(Modes::Pan, NamedMode("Pan", log.clone()));
        });
        ctx.connect_surface_mode_managers();
        ctx.with_surface(1, |ctx| ctx.set_active_mode(Modes::Pan));
        for index in 0..3 {
            assert!(ctx.with_surface(index, |ctx| ctx.modes.is_active(Modes::Pan)));
        }
        assert_eq!(log.borrow().len(), 3);
        ctx.set_temporary_mode(Modes::Volume);
        assert!(ctx.with_surface(2, |ctx| ctx.modes.is_active(Modes::Volume)));
        ctx.restore_mode();
        for index in 0..3 {
            assert!(ctx.with_surface(index, |ctx| ctx.modes.is_active(Modes::Pan)));
        }
    }
}
