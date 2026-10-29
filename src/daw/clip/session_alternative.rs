// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Former `ISessionAlternative`.

/// Additional methods for keeping track of alternative clip/scene interactions (e.g. a clip
/// or scene pad which was used in combination with Shift).
pub trait SessionAlternative {
    /// Check for the alternate interaction flag. True if the Shift button was used in
    /// combination with a clip or scene.
    fn was_alternate_interaction_used(&self) -> bool;

    /// Set or clear the alternate interaction flag.
    fn set_alternate_interaction_used(&mut self, was_used: bool);
}
