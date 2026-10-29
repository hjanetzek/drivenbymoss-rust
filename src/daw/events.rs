// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Events recorded by the model for the controller layer (replaces the Java observers).
//!
//! The model records events while it is updated (by the parser or by commands) and the
//! controller layer drains them with [`crate::daw::model::Model::take_events`], e.g. once per
//! flush, and dispatches them to the views/modes which registered observers in Java.

/// Identifies a parameter bank of the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParameterBankId {
    /// The parameter bank of the cursor device.
    CursorDevice,
    /// The parameter bank of the first instrument device.
    FirstInstrument,
    /// The parameter bank of the equalizer device.
    Eq,
    /// The parameter bank of the drum device (main drum device if page size is None).
    DrumDevice,
    /// The (FX) parameter bank of the selected track.
    CursorTrack,
    /// The project (master FX) parameter bank.
    Project,
}

/// An event recorded by the model.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelEvent {
    /// The selection state of a track changed (former track bank selection observers).
    /// `index` is the index of the track in the page of the track bank, -1 for the master track.
    TrackSelectionChanged { index: i32, is_selected: bool },
    /// The page of the track bank changed (former page observers of the track bank).
    TrackBankPageChanged,
    /// The page of the scene bank changed.
    SceneBankPageChanged,
    /// The page of the send bank of the track at the given position changed.
    SendBankPageChanged { track_position: i32 },
    /// A note is played on a track (velocity 0 = note off; former `INoteObserver`). Only sent
    /// for tracks of the current page; `track_index` is the index in the page.
    TrackNote { track_index: i32, note: i32, velocity: i32 },
    /// A parameter value was received from the DAW (former `IParameterValueObserver`).
    ParameterValueChanged { bank: ParameterBankId, page: i32, index: i32 },
    /// The page of a parameter bank changed.
    ParameterPageChanged { bank: ParameterBankId },
    /// The current track bank was toggled between instrument/audio and effect tracks.
    CurrentTrackBankChanged,
}
