// Copyright (c) 2018-2026 by Jürgen Moßgraber (www.mossgrabers.de)
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#include "stdafx.h"

#define REAPERAPI_IMPLEMENT

#include <algorithm>
#include <array>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <numeric>
#include <set>
#include <sstream>
#include <thread>
#include <unordered_map>
#include <wdltypes.h>
#include <lineparse.h>

#include "resource.h"

#include "CodeAnalysis.h"
#include "DrivenByMossSurface.h"
#include "LocalMidiEventDispatcher.h"
#include "MidiProcessingStructures.h"
#include "ReaDebug.h"
#include "StringUtils.h"

midi_Input* (*GetMidiInput)(int idx);
midi_Output* (*GetMidiOutput)(int idx);

// The global extension variables required to bridge from C to C++,
// static keyword restricts the visibility of a function to the file
REAPER_PLUGIN_HINSTANCE pluginInstanceHandle = nullptr;
gaccel_register_t openDBMConfigureWindowAccel = { {0,0,0}, "DrivenByMoss: Open the configuration window." };
gaccel_register_t openDBMProjectWindowAccel = { {0,0,0}, "DrivenByMoss: Open the project settings window." };
gaccel_register_t openDBMParameterWindowAccel = { {0,0,0}, "DrivenByMoss: Open the parameter settings window." };
gaccel_register_t restartControllersAccel = { {0,0,0}, "DrivenByMoss: Restart all controllers." };

// The DrivenByMoss instance (Rust library)
std::unique_ptr<dbm::App> dbmApp;

// MIDI port handling
std::set<int> activeMidiOutputs;
std::set<int> activeMidiInputs;

// The audio hook for MIDI communication
audio_hook_register_t audioHook;

// Atomic shared snapshot (lock-free access)
using DeviceMap = std::unordered_map<int, DeviceNoteData>;
std::shared_ptr<DeviceMap> deviceDataSnapshot;
std::mutex deviceDataMutex; // only needed for writes, not reads

// DrivenByMoss to internal Reaper
LocalMidiEventDispatcher localMidiEventDispatcher{};

// Defined in DrivenByMossSurface.cpp
extern DrivenByMossSurface* surfaceInstance;


/**
 * Convert hex string to vector of unsigned char.
 *
 * @param hexStr The hex codes to parse, needs to be an equal number of characters
 * @return The parsed pair-characters as byte numbers
 */
static std::vector<unsigned char> ParseHexFilter(const std::string& hexStr)
{
	std::vector<unsigned char> result;
	for (size_t i = 0; i < hexStr.length(); i += 2)
	{
		std::string byteStr = hexStr.substr(i, 2);
		unsigned int byteVal;
		std::stringstream ss;
		ss << std::hex << byteStr;
		ss >> byteVal;
		result.push_back(static_cast<unsigned char>(byteVal));
	}
	return result;
}


/**
 * Update the note data of a device in the shared snapshot (copy on write).
 *
 * @param deviceID The ID of the MIDI input
 * @param update   The function which modifies the device data
 */
template <typename F>
static void UpdateDeviceData(int deviceID, F update)
{
	const std::lock_guard<std::mutex> lock(deviceDataMutex);
	DISABLE_WARNING_PREFER_LOCAL_OBJECTS
	std::shared_ptr<DeviceMap> current = std::atomic_load(&deviceDataSnapshot);
	if (!current)
		current = std::make_shared<DeviceMap>();
	std::shared_ptr<DeviceMap> updated = std::make_shared<DeviceMap>(*current);
	update((*updated)[deviceID]);
	std::atomic_store(&deviceDataSnapshot, updated);
}


/**
 * The functions which DrivenByMoss (Rust library) calls in Reaper. Replaces the former JNI
 * callbacks. All functions are called on the main thread (the library is ticked from
 * DrivenByMossSurface::Run).
 */
class ReaperBackend : public dbm::Backend
{
public:
	void processNoArg(const std::string& processor, const std::string& command) override
	{
		if (surfaceInstance != nullptr)
			surfaceInstance->GetOscParser().Process(processor, command);
	}

	void processStringArg(const std::string& processor, const std::string& command, const std::string& value) override
	{
		if (surfaceInstance != nullptr)
			surfaceInstance->GetOscParser().Process(processor, command, value);
	}

	void processStringArgs(const std::string& processor, const std::string& command, const std::vector<std::string>& values) override
	{
		if (surfaceInstance == nullptr)
			return;
		std::vector<std::string> parameters(values);
		surfaceInstance->GetOscParser().Process(processor, command, parameters);
	}

	void processIntArg(const std::string& processor, const std::string& command, int value) override
	{
		if (surfaceInstance != nullptr)
			surfaceInstance->GetOscParser().Process(processor, command, value);
	}

	void processDoubleArg(const std::string& processor, const std::string& command, double value) override
	{
		if (surfaceInstance != nullptr)
			surfaceInstance->GetOscParser().Process(processor, command, value);
	}

	void enableUpdates(const std::string& processor, bool enable) override
	{
		if (surfaceInstance != nullptr)
			surfaceInstance->GetDataCollector().EnableUpdate(processor, enable);
	}

	// Use to prevent that Reaper sends old values before the latest ones are applied
	void delayUpdates(const std::string& processor) override
	{
		if (surfaceInstance != nullptr)
			surfaceInstance->GetDataCollector().DelayUpdate(processor);
	}

	// Send a MIDI message to Reaper as if it was received on the MIDI input
	void processMidiArg(int deviceID, int status, int data1, int data2) override
	{
		MIDI_event_t event{};
		event.frame_offset = 0;
		event.size = 3;
		event.midi_message[0] = gsl::narrow_cast<unsigned char>(status);
		event.midi_message[1] = gsl::narrow_cast<unsigned char>(data1);
		event.midi_message[2] = gsl::narrow_cast<unsigned char>(data2);
		localMidiEventDispatcher.Push(deviceID, event);
	}

	// All MIDI inputs which are enabled in Reaper
	std::vector<dbm::MidiDevice> getMidiInputs() override
	{
		std::vector<dbm::MidiDevice> devices;
		const int numMidiInputs = GetNumMIDIInputs();
		for (int deviceID = 0; deviceID < numMidiInputs; deviceID++)
		{
			if (!GetMidiInput(deviceID))
				continue;
			char buf[512];
			DISABLE_WARNING_ARRAY_POINTER_DECAY
			if (GetMIDIInputNameNoAlias(deviceID, buf, sizeof(buf)))
				devices.push_back({ deviceID, buf });
		}
		return devices;
	}

	// All MIDI outputs which are enabled in Reaper
	std::vector<dbm::MidiDevice> getMidiOutputs() override
	{
		std::vector<dbm::MidiDevice> devices;
		const int numMidiOutputs = GetNumMIDIOutputs();
		for (int deviceID = 0; deviceID < numMidiOutputs; deviceID++)
		{
			if (!GetMidiOutput(deviceID))
				continue;
			char buf[512];
			DISABLE_WARNING_ARRAY_POINTER_DECAY
			if (GetMIDIOutputNameNoAlias(deviceID, buf, sizeof(buf)))
				devices.push_back({ deviceID, buf });
		}
		return devices;
	}

	bool openMidiInput(int deviceID) override
	{
		if (deviceID < 0 || deviceID >= GetNumMIDIInputs())
			return false;
		activeMidiInputs.insert(deviceID);
		return true;
	}

	bool openMidiOutput(int deviceID) override
	{
		if (deviceID < 0 || deviceID >= GetNumMIDIOutputs())
			return false;
		activeMidiOutputs.insert(deviceID);
		return true;
	}

	void closeMidiInput(int deviceID) override
	{
		activeMidiInputs.erase(deviceID);
	}

	void closeMidiOutput(int deviceID) override
	{
		activeMidiOutputs.erase(deviceID);
	}

	// Queue a MIDI message for a MIDI output port, sent from DrivenByMossSurface::Run
	void sendMidiData(int deviceID, const uint8_t* data, int length) override
	{
		if (surfaceInstance == nullptr || data == nullptr || length <= 0)
			return;

		if (length <= 3)
		{
			// ---------- 1‑3 bytes ----------
			const gsl::span<const uint8_t> spanSource(data, length);
			std::unique_ptr<Midi3> m = std::make_unique<Midi3>();
			m->deviceId = gsl::narrow_cast<uint32_t>(deviceID);
			m->status = gsl::at(spanSource, 0);
			m->data1 = length > 1 ? gsl::at(spanSource, 1) : 0;
			m->data2 = length > 2 ? gsl::at(spanSource, 2) : 0;
			surfaceInstance->outgoingMidiQueue3.push(std::move(m));
		}
		else if (length <= kSyx1k_Max)
		{
			// ---------- ≤ 1 024 bytes ----------
			std::unique_ptr<MidiSyx1k> m = std::make_unique<MidiSyx1k>();
			m->deviceId = gsl::narrow_cast<uint32_t>(deviceID);
			m->size = gsl::narrow_cast<uint32_t>(length);
			std::memcpy(m->data, data, length);
			surfaceInstance->outgoingMidiQueue1k.push(std::move(m));
		}
		else if (length <= kSyx64k_Max)
		{
			// ---------- ≤ 65 536 bytes ----------
			std::unique_ptr<MidiSyx64k> m = std::make_unique<MidiSyx64k>();
			m->deviceId = gsl::narrow_cast<uint32_t>(deviceID);
			m->size = gsl::narrow_cast<uint32_t>(length);
			std::memcpy(m->data, data, length);
			surfaceInstance->outgoingMidiQueue64k.push(std::move(m));
		}
		else
			ReaDebug::Log("DrivenByMoss: Attempt to send MIDI message greater 64k.\n");
	}

	// Set the MIDI filters for a note input
	void setNoteInputFilters(int deviceID, int noteInputIndex, const std::vector<std::string>& filters) override
	{
		if (noteInputIndex < 0 || noteInputIndex >= static_cast<int>(MAX_NOTE_INPUTS))
			return;

		FilterSet parsed;
		for (std::string hex : filters)
		{
			// This parser expects 2 or 4 charcters!
			hex.erase(std::remove_if(hex.begin(), hex.end(), ::isspace), hex.end());
			if (hex.length() == 2 || hex.length() == 4)
				parsed.push_back(ParseHexFilter(hex));
		}

		UpdateDeviceData(deviceID, [&](DeviceNoteData& deviceData) {
			deviceData.noteInputs[noteInputIndex].filters = std::move(parsed);
			deviceData.BuildFilterLookup();
		});
	}

	// Set the MIDI key translation table (128 elements) for a note input
	void setNoteInputKeyTranslationTable(int deviceID, int noteInputIndex, const std::vector<int>& table) override
	{
		std::array<int, 128> parsed;
		if (!CopyTable(noteInputIndex, table, parsed))
			return;
		UpdateDeviceData(deviceID, [&](DeviceNoteData& deviceData) {
			deviceData.noteInputs[noteInputIndex].keyTable = parsed;
			deviceData.BuildKeyLookup();
		});
	}

	// Set the MIDI velocity translation table (128 elements) for a note input
	void setNoteInputVelocityTranslationTable(int deviceID, int noteInputIndex, const std::vector<int>& table) override
	{
		std::array<int, 128> parsed;
		if (!CopyTable(noteInputIndex, table, parsed))
			return;
		UpdateDeviceData(deviceID, [&](DeviceNoteData& deviceData) {
			deviceData.noteInputs[noteInputIndex].velocityTable = parsed;
			deviceData.BuildVelocityLookup();
		});
	}

private:
	static bool CopyTable(int noteInputIndex, const std::vector<int>& table, std::array<int, 128>& out)
	{
		if (noteInputIndex < 0 || noteInputIndex >= static_cast<int>(MAX_NOTE_INPUTS) || table.size() != out.size())
			return false;
		std::copy(table.begin(), table.end(), out.begin());
		return true;
	}
};

ReaperBackend reaperBackend;


/**
 * Create the DrivenByMoss instance if it does not exist yet. The instance is ticked from
 * DrivenByMossSurface::Run, therefore all callbacks happen on the main thread.
 */
static void CreateApp()
{
	if (dbmApp != nullptr)
		return;
	ReaDebug() << "DrivenByMoss: Starting library version " << dbm::version() << ".";
	dbmApp = std::make_unique<dbm::App>(reaperBackend, GetResourcePath(), GetAppVersion(), DBM_FLAG_HOST_TICK);
	if (!dbmApp->isValid())
	{
		ReaDebug::Log("DrivenByMoss: Could not create the DrivenByMoss instance.\n");
		dbmApp.reset();
	}
}


/**
 * Callback for custom (keyboard) actions.
 *
 * @param command The ID of the command to execute
 * @param flag    Execution flag, not used
 */
static bool HookCommandProc(int command, int flag)
{
	if (!dbmApp)
		return false;

	if (openDBMConfigureWindowAccel.accel.cmd != 0 && openDBMConfigureWindowAccel.accel.cmd == command)
	{
		dbmApp->displayWindow();
		return true;
	}
	if (openDBMProjectWindowAccel.accel.cmd != 0 && openDBMProjectWindowAccel.accel.cmd == command)
	{
		dbmApp->displayProjectWindow();
		return true;
	}
	if (openDBMParameterWindowAccel.accel.cmd != 0 && openDBMParameterWindowAccel.accel.cmd == command)
	{
		dbmApp->displayParameterWindow();
		return true;
	}
	if (restartControllersAccel.accel.cmd != 0 && restartControllersAccel.accel.cmd == command)
	{
		dbmApp->restartControllers();
		return true;
	}
	return false;
}


/**
 * (Windows) message callback of the configuration dialog.
 *
 * @param hwndDlg The handle of the dialog window
 * @param uMsg    The message to handle
 * @param wParam  The int parameter
 * @param lParam  The long parameter
 */
static WDL_DLGRET dlgProc(HWND hwndDlg, UINT uMsg, WPARAM wParam, LPARAM lParam)
{
	switch (uMsg)
	{
	case WM_INITDIALOG:
	{
		std::string path = "DrivenByMoss " + dbm::version();
#ifdef _WIN32
		SetDlgItemText(hwndDlg, IDC_JAVA_HOME, stringToWs(path).c_str());
#else
		SetDlgItemText(hwndDlg, IDC_JAVA_HOME, path.c_str());
#endif
		ShowWindow(GetDlgItem(hwndDlg, IDC_REOPEN_INFO), SW_HIDE);
	}
	break;

	// Display the configuration window
	case WM_COMMAND:
	{
		DISABLE_WARNING_NO_C_STYLE_CONVERSION
		const WORD value = LOWORD(wParam);
		switch (value)
		{
		case IDC_BUTTON_CONFIGURE:
			if (dbmApp)
				dbmApp->displayWindow();
			break;
		default:
			// Ignore the rest
			break;
		}
	}
	break;

	default:
		// Ignore the rest
		break;
	}
	return 0;
}


/**
 * Callback function for Reaper to create an instance of the extension.
 *
 * @param type_string  Unused
 * @param configString Unused
 * @param errStats     Unused
 */
static IReaperControlSurface* createFunc(const char* type_string, const char* configString, int* errStats)
{
	if (!ENABLE_EXTENSION)
		return nullptr;

	// Note: If the setup dialog is closed with OK, the current surfaceInstance will be destructed
	// but the DrivenByMoss instance is kept
	CreateApp();

	ReaDebug::Log("DrivenByMoss: Creating surface.\n");

	// Note: delete is called from Reaper on shutdown, no need to do it ourselves
	DISABLE_WARNING_DONT_USE_NEW
	surfaceInstance = new DrivenByMossSurface(dbmApp, GetMidiOutput);

	ReaDebug::Log("DrivenByMoss: Surface created.\n");

	return surfaceInstance;
}

// Callback function for Reaper to create the configuration dialog of the extension
static HWND configFunc(const char* type_string, HWND parent, const char* initConfigString) noexcept
{
	// No way to prevent the LPARAM cast
	DISABLE_WARNING_REINTERPRET_CAST
	return CreateDialogParam(pluginInstanceHandle, MAKEINTRESOURCE(IDD_SURFACEEDIT_DRIVENBYMOSS), parent, dlgProc, reinterpret_cast<LPARAM>(initConfigString));
}

// Description for DrivenByMoss surfaceInstance extension
static reaper_csurf_reg_t drivenbymoss_reg =
{
	"DrivenByMoss4Reaper",
	"DrivenByMoss4Reaper",
	createFunc,
	configFunc,
};


/**
 * Called for all extension sub-tags of the project file to parse additional parameters which are specific to an extension.
 *
 * @param line   The line to parse
 * @param ctx    The project context (not used)
 * @param isUndo Is this called due to an undo operation? (not used)
 * @param reg    Pointer to the registered extension (not used)
 */
static bool ProcessExtensionLine(const char* line, ProjectStateContext* ctx, bool isUndo, struct project_config_extension_t* reg)
{
	ReaDebug::Log("DrivenByMoss: Processing project parameters.\n");

	if (ctx == nullptr)
		return false;

	if (!dbmApp)
		return false;

	// Parse the line and check if it is valid and belongs to this extension
	LineParser lp(false);
	if (lp.parse(line) || lp.getnumtokens() < 1 || strcmp(lp.gettoken_str(0), "<DRIVEN_BY_MOSS") != 0)
	{
		ReaDebug::Log("DrivenByMoss: Processing project parameters done - none found.\n");
		return false;
	}

	constexpr int LENGTH = 8000;
	std::string data(LENGTH, 0);
	char* dataPointer = &*data.begin();
	while (true)
	{
		if (ctx->GetLine(dataPointer, LENGTH) || lp.parse(dataPointer))
			break;

		const char* token = lp.gettoken_str(0);
		if (token != nullptr)
		{
			std::string tokenStr = token;
			if (tokenStr.at(0) == '>')
				break;
		}

		dbmApp->setFormattedDocumentSettings(data.c_str());
	}

	ReaDebug::Log("DrivenByMoss: Processing project parameters done.\n");
	return true;
}


/**
 * Callback for storing additional parameters for the registered extension.
 *
 * @param ctx    The project context (not used)
 * @param isUndo Is this called due to an undo operation? (not used)
 * @param reg    Pointer to the registered extension (not used)
 */
static void SaveExtensionConfig(ProjectStateContext* ctx, bool isUndo, struct project_config_extension_t* reg)
{
	ReaDebug::Log("DrivenByMoss: Saving project settings.\n");

	if (ctx == nullptr)
		return;

	if (!dbmApp)
		return;

	std::string line = dbmApp->getFormattedDocumentSettings();

	ctx->AddLine("<DRIVEN_BY_MOSS");
	ctx->AddLine("%s", line.c_str());
	ctx->AddLine(">");

	ReaDebug::Log("DrivenByMoss: Project settings saved.\n");
}


/**
 * Callback for loading additional parameters for the registered extension.
 *
 * @param isUndo Is this called due to an undo operation? (not used)
 * @param reg    Pointer to the registered extension (not used)
 */
static void BeginLoadProjectState(bool isUndo, struct project_config_extension_t* reg)
{
	ReaDebug::Log("DrivenByMoss: Loading project settings.\n");

	// Called on project load/undo before any (possible) ProcessExtensionLine. NULL is OK too
	// also called on "new project" (wont be followed by ProcessExtensionLine calls in that case)
	// Defaults could be set here but are already set by the controller instances
	if (dbmApp)
		dbmApp->setDefaultDocumentSettings();

	ReaDebug::Log("DrivenByMoss: Project settings loaded.\n");
}

// Structure for registering the project notification extension
project_config_extension_t pcreg =
{
  ProcessExtensionLine,
  SaveExtensionConfig,
  BeginLoadProjectState,
  nullptr,
};


/**
 * Matches the incoming MIDI event against the registered note input filters.
 *
 * @param deviceData The ID of the device to which to match the event
 * @param event The event to match/filter
 */
inline static bool ProcessMidiEvents(const DeviceNoteData& deviceData, MIDI_event_t* event) noexcept
{
	const unsigned char data1 = event->size > 1 ? event->midi_message[1] : 0;
	if (data1 >= 128)
		return false;

	// Do not use gsl:at for performance reasons!
	DISABLE_WARNING_USE_GSL_AT
	DISABLE_WARNING_ACCESS_ARRAYS_WITH_CONST

	const unsigned char status = event->midi_message[0];
	const int statusType = status & 0xF0;
	const bool isNote = statusType == 0x90 || statusType == 0x80 || statusType == 0xA0;

	for (size_t noteIdx = 0; noteIdx < MAX_NOTE_INPUTS; ++noteIdx)
	{
		// Note: to be 100% correct this would require the creation of a new MIDI event since
		// theoretically multiple note inputs could be present and events could be modified differently
		// If this becomes a use-case it would need to be implemented with a pre-allocated pool or ring 
		// buffer of MIDI_event_t

		if (deviceData.filterMatch[noteIdx][status][data1])
		{
			if (isNote)
			{
				if (deviceData.keyLookup[noteIdx][data1] < 0)
					continue;

				event->midi_message[1] = deviceData.keyLookup[noteIdx][data1];

				const unsigned char data2 = event->size > 2 ? event->midi_message[2] : 0;

				if (deviceData.velocityLookup[noteIdx][data2] < 0)
					continue;

				if (data2 >= 128)
					return false;

				event->midi_message[2] = deviceData.velocityLookup[noteIdx][data2];
			}

			return true;
		}
	}

	return false;
}


static void OnExit() noexcept
{
	if (surfaceInstance != nullptr)
		surfaceInstance->Shutdown();
}


/**
 * Audio hook. Reads from registered MIDI inputs and queues the events to be sent to DrivenByMoss as well
 * as matching them against the registered note input filters and sends the matches to Reaper.
 * Adds additional MIDI events intended for Reaper.
 * The method is called before and after the update of the audio buffer
 *
 * @param isPost True if the call is after the update of the audio buffer
 * @param len    The length of the buffer (not used)
 * @param srate  The sample rate (not used)
 * @param reg    Pointer to the registered audio hook structure (not used)
 */
static void OnAudioBuffer(bool isPost, int len, double srate, struct audio_hook_register_t* reg)
{
	if (surfaceInstance == nullptr || isPost)
		return;

	for (const auto& deviceID : activeMidiInputs)
	{
		midi_Input* midiin = GetMidiInput(deviceID);
		if (!midiin)
			continue;

		auto snapshot = std::atomic_load(&deviceDataSnapshot);
		if (!snapshot)
			continue;

		const auto deviceIt = snapshot->find(deviceID);
		if (deviceIt == snapshot->end())
			continue;

		MIDI_eventlist* list = midiin->GetReadBuf();
		if (!list)
			continue;

		const auto& deviceData = deviceIt->second;

		int position = 0;
		int nextPosition = 0;
		MIDI_event_t* event;
		// Copy the events out of the audio thread to be sent to DrivenByMoss
		while ((event = list->EnumItems(&nextPosition)) != nullptr)
		{
			const int size = event->size;
			if (size == 0)
				continue;

			if (size > 3)
			{
				if (size < 1024)
					surfaceInstance->EnqueueSysex1k(deviceID, event->midi_message, event->size);
				else
					surfaceInstance->EnqueueSysex64k(deviceID, event->midi_message, event->size);
				continue;
			}

			const uint8_t status = event->midi_message[0];
			// Ignore active sensing
			if (status == 0 || status == 0xFE)
				continue;

			const uint8_t data1 = (size > 1) ? event->midi_message[1] : 0;
			const uint8_t data2 = (size > 2) ? event->midi_message[2] : 0;
			surfaceInstance->EnqueueMidi3(deviceID, status, data1, data2);
			// Apply note input filters
			if (!ProcessMidiEvents(deviceData, event))
			{
				list->DeleteItem(position);
				nextPosition = position;
			}
			else
				position = nextPosition;
		}

		// Add events to be sent to Reaper
		localMidiEventDispatcher.ProcessDeviceQueue(deviceID, list);
	}
}


static bool RegisterAccelerator(gaccel_register_t& accelerator, const reaper_plugin_info_t* rec, const char* id, const char* errorMsg) noexcept
{
	if (rec == nullptr)
		return false;
	accelerator.accel.cmd = rec->Register("command_id", (void*)id);
	if (!accelerator.accel.cmd)
	{
		ReaDebug() << "Could not register ID for DrivenByMoss " << errorMsg <<  " action.";
		return false;
	}
	if (!rec->Register("gaccel", &accelerator))
	{
		ReaDebug() << "Could not register DrivenByMoss " << errorMsg << ".";
		return false;
	}
	return true;
}


// Must be extern to be exported from the DLL
extern "C"
{
	// Defines the entry point for the DLL application.
	// It is always executed after Reaper loaded it, even if the extension is not added in the configuration dialog!
	REAPER_PLUGIN_DLL_EXPORT int REAPER_PLUGIN_ENTRYPOINT(REAPER_PLUGIN_HINSTANCE hInstance, const reaper_plugin_info_t* rec)
	{
		if (!ENABLE_EXTENSION)
			return 0;

		// On shutdown
		if (rec == nullptr)
			return 0;

		// On startup...

		pluginInstanceHandle = hInstance;
		ReaperUtils::mainWindowHandle = rec->hwnd_main;
		deviceDataSnapshot = std::make_shared<DeviceMap>();

		if (rec->caller_version != REAPER_PLUGIN_VERSION || rec->GetFunc == nullptr)
			return 0;

		// False positive, null check above is not detected
		DISABLE_WARNING_DANGLING_POINTER
		REAPERAPI_LoadAPI(rec->GetFunc);

		// Register extension
		const int result = rec->Register("csurf", &drivenbymoss_reg);
		if (!result)
		{
			ReaDebug() << "Could not instantiate DrivenByMoss surfaceInstance extension.";
			return 0;
		}

		// Register the shutdown callback
		plugin_register("atexit", (void*)OnExit);

		// Register project notifications
		rec->Register("projectconfig", &pcreg);

		// Register actions
		RegisterAccelerator(openDBMConfigureWindowAccel, rec, "DBM_OPEN_WINDOW_ACTION", "open configuration window");
		RegisterAccelerator(openDBMProjectWindowAccel, rec, "DBM_OPEN_PROJECT_WINDOW_ACTION", "open project window");
		RegisterAccelerator(openDBMParameterWindowAccel, rec, "DBM_OPEN_PARAMETER_WINDOW_ACTION", "parameter mapping window");
		RegisterAccelerator(restartControllersAccel, rec, "RESTART_CONTROLLERS_ACTION", "restart controllers");

		if (!rec->Register("hookcommand", (void*)HookCommandProc))
		{
			ReaDebug() << "Could not register DrivenByMoss action callback.";
			return 0;
		}

		// Make undocumented GetMidiInput and GetMidiOutput functions available
		GetMidiInput = (midi_Input * (*)(int))rec->GetFunc("GetMidiInput");
		if (!GetMidiInput)
		{
			ReaDebug() << "GetMidiInput is not available.";
			return 0;
		}
		GetMidiOutput = (midi_Output * (*)(int))rec->GetFunc("GetMidiOutput");
		if (!GetMidiOutput)
		{
			ReaDebug() << "GetMidiOutput is not available.";
			return 0;
		}

		// Register audio hook
		audioHook.userdata1 = nullptr;
		audioHook.userdata2 = nullptr;
		audioHook.input_nch = 0;
		audioHook.output_nch = 0;
		audioHook.GetBuffer = nullptr;
		audioHook.OnAudioBuffer = OnAudioBuffer;
		const int hookResult = Audio_RegHardwareHook(true, &audioHook);
		if (!hookResult)
		{
			ReaDebug() << "AudioHook could not be registered!.";
			return 0;
		}

		return 1;
	}
};


#ifndef _WIN32 // MAC resources
#include "swell-dlggen.h"
#include "res.rc_mac_dlg"
#include "dllmain.h"
#undef BEGIN
#undef END
#endif
