// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// Test of the C++ wrapper: drives a Launchpad X instance through dbm::App with a recording
// backend. Built and run by run_tests.sh (compile-checked by tests/ffi.rs).

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include "drivenbymoss.hpp"

static int failures = 0;

#define CHECK(condition) \
    do { if (!(condition)) { std::fprintf (stderr, "FAILED: %s (line %d)\n", #condition, __LINE__); failures++; } } while (0)


class RecordingBackend : public dbm::Backend
{
public:
    std::vector<std::string> commands;
    std::vector<std::pair<int, std::vector<uint8_t>>> midiOut;
    std::vector<int> opened;
    std::vector<std::string> logs;
    bool throwOnCommand{ false };

    void log (const std::string &message) override { this->logs.push_back (message); }

    void processNoArg (const std::string &processor, const std::string &command) override
    {
        if (this->throwOnCommand)
            throw std::runtime_error ("Must not escape into the library");
        this->commands.push_back (processor + " " + command);
    }

    void processIntArg (const std::string &processor, const std::string &command, int value) override
    {
        this->commands.push_back (processor + " " + command + " " + std::to_string (value));
    }

    std::vector<dbm::MidiDevice> getMidiInputs () override { return { { 3, "Launchpad X LPX MIDI Out" } }; }
    std::vector<dbm::MidiDevice> getMidiOutputs () override { return { { 5, "Launchpad X LPX MIDI In" } }; }

    bool openMidiInput (int deviceId) override { this->opened.push_back (deviceId); return true; }
    bool openMidiOutput (int deviceId) override { this->opened.push_back (deviceId); return true; }

    void sendMidiData (int deviceId, const uint8_t *data, int length) override
    {
        this->midiOut.emplace_back (deviceId, std::vector<uint8_t> (data, data + length));
    }

    int padUpdates () const
    {
        int count = 0;
        for (const auto &message : this->midiOut)
            if (message.first == 5 && !message.second.empty () && message.second[0] == 0x90)
                count++;
        return count;
    }
};


int main (int argc, char **argv)
{
    const std::string dir = argc > 1 ? argv[1] : ".";
    std::ofstream (dir + "/DrivenByMoss4Reaper.config") << "CONTROLLER_INSTANCE0=de.mossgrabers.reaper.controller.novation.launchpad.LaunchpadXControllerInstance\n";

    CHECK (!dbm::version ().empty ());

    RecordingBackend backend;
    {
        dbm::App app (backend, dir, "7.22/linux-x86_64", DBM_FLAG_HOST_TICK);
        CHECK (app.isValid ());

        app.startInfrastructure ();
        CHECK (backend.opened == (std::vector<int>{ 3, 5 }));

        app.updateModel ("track/count 1\ntrack/0/exists 1\ntrack/0/select 1\n");
        backend.midiOut.clear ();
        app.tick ();
        CHECK (backend.padUpdates () >= 64);

        // Pad press
        const uint8_t noteOn[3] = { 0x90, 11, 127 };
        app.onMidiEvent (3, noteOn, 3);

        // Exceptions in the backend are caught by the wrapper
        backend.throwOnCommand = true;
        app.restartControllers ();
        app.tick ();
        backend.throwOnCommand = false;

        const std::string settings = app.getFormattedDocumentSettings ();
        CHECK (settings.rfind ("LAUNCHPAD_X=\"", 0) == 0);
        app.setFormattedDocumentSettings (settings);

        // Move semantics
        dbm::App moved (std::move (app));
        CHECK (moved.isValid ());
        CHECK (!app.isValid ());
        moved.tick ();
    }

    // A single controller on the ports 0 (as for Bitwig)
    CHECK (dbm::listControllers ().find ("\"model\":\"Launch Control XL\"") != std::string::npos);
    {
        RecordingBackend single;
        dbm::App unknown = dbm::App::forController (single, "unknown", dir, "", DBM_FLAG_HOST_TICK);
        CHECK (!unknown.isValid ());
        dbm::App app = dbm::App::forController (single, "5AFA5045-FF92-4737-A091-5B371CD5E529", dir, "Bitwig Studio 5.3", DBM_FLAG_HOST_TICK);
        CHECK (app.isValid ());
        CHECK (app.getModelSetup ().empty ());
        app.startInfrastructure ();
        CHECK (single.opened == (std::vector<int>{ 0, 0 }));
        CHECK (app.getModelSetup ().find ("\"numSends\":2") != std::string::npos);
        CHECK (!single.logs.empty ());
    }

    // A second, empty backend with all default (no-op) functions
    dbm::Backend empty;
    {
        dbm::App app (empty, dir + "/does-not-exist", "", DBM_FLAG_HOST_TICK);
        CHECK (app.isValid ());
        app.startInfrastructure ();
        app.tick ();
    }

    if (failures == 0)
        std::printf ("All C++ wrapper tests passed.\n");
    return failures == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
