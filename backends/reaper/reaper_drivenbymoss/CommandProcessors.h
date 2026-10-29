// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#ifndef _DBM_COMMANDPROCESSORS_H_
#define _DBM_COMMANDPROCESSORS_H_

#include <map>
#include <string>

#include "OscProcessor.h"
#include "ReaperUtils.h"


// The action IDs are static class members; '+' makes them values, binding them to the references
// of std::pair would require a definition.

/**
 * Executes commands by invoking Reaper actions. The commands replace the Reaper action IDs the
 * core used to send (protocol v2, see doc/BACKEND_PROTOCOL.md); unknown commands are ignored,
 * e.g. the ones for features Reaper does not have.
 */
class ActionCommandProcessor : public OscProcessor
{
public:
	ActionCommandProcessor(Model& aModel, std::map<std::string, int> anActions) : OscProcessor(aModel), actions(std::move(anActions)) {};

	void Process(std::deque<std::string>& path) override
	{
		if (path.empty())
			return;
		const auto action = this->actions.find(path.front());
		if (action != this->actions.end())
			Main_OnCommandEx(action->second, 0, ReaperUtils::GetProject());
	};

	void Process(std::deque<std::string>& path, const std::string& value) noexcept override {};
	void Process(std::deque<std::string>& path, const std::vector<std::string>& values) noexcept override {};
	void Process(std::deque<std::string>& path, double value) noexcept override {};

protected:
	static void Invoke(int action)
	{
		Main_OnCommandEx(action, 0, ReaperUtils::GetProject());
	}

private:
	const std::map<std::string, int> actions;
};


/**
 * The "transport" commands.
 */
class TransportCommandProcessor : public ActionCommandProcessor
{
public:
	explicit TransportCommandProcessor(Model& aModel) : ActionCommandProcessor(aModel, {
		{ "toggleMetronome", +TOGGLE_METRONOME },
		{ "togglePrerollMetronome", +RECORD_PREROLL },
		{ "tapTempo", +TRANSPORT_TAP_TEMPO },
		// Transport: Go to end of project
		{ "end", 40043 }
	}) {};

	using ActionCommandProcessor::Process;

	void Process(std::deque<std::string>& path, int value) override
	{
		const std::string command = path.empty() ? "" : path.front();
		if (command == "metronome")
			Invoke(value > 0 ? ENABLE_METRONOME : DISABLE_METRONOME);
		// Punch in: auto-punch with the time selection (loop), punch out: selected items
		else if (command == "punchIn")
			Invoke(value > 0 ? RECORD_MODE_AUTO_PUNCH : RECORD_MODE_NORMAL);
		else if (command == "punchOut")
			Invoke(value > 0 ? RECORD_MODE_PUNCH_ITEMS : RECORD_MODE_NORMAL);
		else
			ActionCommandProcessor::Process(path, value);
	};
};


/**
 * The "application" commands.
 */
class ApplicationProcessor : public ActionCommandProcessor
{
public:
	explicit ApplicationProcessor(Model& aModel) : ActionCommandProcessor(aModel, {
		{ "toggleNoteEditor", +TOGGLE_SHOW_MIDI_EDITOR_WINDOWS },
		{ "toggleAutomationEditor", +SHOW_ALL_ACTIVE_ENVELOPES },
		{ "toggleInspector", +SHOW_TRACK_MANAGER_WINDOW },
		{ "toggleMixer", +TOGGLE_MIXER_VISIBLE },
		{ "toggleFullScreen", +TOGGLE_FULLSCREEN },
		{ "toggleBrowser", +TOGGLE_MEDIA_EXPLORER },
		{ "deleteSelection", +REMOVE_ITEMS },
		{ "arrowLeft", +GO_TO_PREV_TRACK },
		{ "arrowUp", +GO_TO_PREV_TRACK },
		{ "arrowRight", +GO_TO_NEXT_TRACK },
		{ "arrowDown", +GO_TO_NEXT_TRACK },
		{ "zoomIn", +ZOOM_IN_HORIZ },
		{ "zoomOut", +ZOOM_OUT_HORIZ },
		{ "incTrackHeight", +ZOOM_IN_VERT },
		{ "decTrackHeight", +ZOOM_OUT_VERT },
		{ "sliceToSampler", +DYNAMIC_SPLIT },
		{ "sliceToDrumMachine", +DYNAMIC_SPLIT }
	}) {};

	using ActionCommandProcessor::Process;

	void Process(std::deque<std::string>& path, int value) override
	{
		// The panel layouts arrange, mix, edit, play are the window sets 1 to 4
		if (!path.empty() && path.front() == "layout")
		{
			if (value >= 0 && value < 4)
				Invoke(LOAD_WINDOW_SET_1 + value);
		}
		else
			ActionCommandProcessor::Process(path, value);
	};
};


/**
 * The "arranger" commands.
 */
class ArrangerProcessor : public ActionCommandProcessor
{
public:
	explicit ArrangerProcessor(Model& aModel) : ActionCommandProcessor(aModel, {
		// View: Toggle show marker lane
		{ "toggleCueMarkers", 42328 },
		// Options: Toggle auto-view-scroll during playback
		{ "togglePlaybackFollow", 40036 },
		// View: Cycle track zoom between minimum, default, and maximum height
		{ "toggleTrackRowHeight", 42698 }
	}) {};
};


/**
 * The "mixer" commands.
 */
class MixerProcessor : public ActionCommandProcessor
{
public:
	explicit MixerProcessor(Model& aModel) : ActionCommandProcessor(aModel, {
		{ "toggleDeviceSection", +TOGGLE_FX_INSERTS },
		{ "toggleIoSection", +TOGGLE_FX_PARAMETERS },
		{ "toggleSendsSection", +TOGGLE_FX_SENDS }
	}) {};
};

#endif /* _DBM_COMMANDPROCESSORS_H_ */
