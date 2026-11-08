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
    /** The commands of the binary form as text, and the typed ones seen by the views. */
    std::vector<std::string> binary;
    int refreshes{ 0 };
    int fxBank{ -1 };

    void processCommands (const uint8_t *buffer, size_t length) override
    {
        this->binary.push_back (dbm::messagesToText (buffer, length));
        for (const dbm::protocol::Record &r : dbm::protocol::Reader (buffer, length))
        {
            switch (r.cmd ())
            {
                case dbm::protocol::Cmd::Refresh:
                    this->refreshes++;
                    break;
                case dbm::protocol::Cmd::BankScroll:
                {
                    const auto c = r.as<dbm::protocol::cmd::BankScroll> ();
                    if (c.bank () == DBM_PROCESSOR_FX_TRACK)
                        this->fxBank = c.value ();
                    break;
                }
                default:
                    break;
            }
        }
    }

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
        CHECK (app.getModelSetup () == "{}");
        app.startInfrastructure ();
        CHECK (single.opened == (std::vector<int>{ 0, 0 }));
        CHECK (app.getModelSetup ().find ("\"numSends\":2") != std::string::npos);
        CHECK (!single.logs.empty ());
    }

    // The binary form of the protocol
    {
        RecordingBackend single;
        dbm::App app = dbm::App::forController (single, "5AFA5045-FF92-4737-A091-5B371CD5E529", dir, "Bitwig Studio 5.3", DBM_FLAG_HOST_TICK);
        CHECK (app.useBinaryCommands ());
        app.startInfrastructure ();
        CHECK (single.commands.empty ());
        CHECK (!single.binary.empty ());
        CHECK (single.refreshes == 1);
        CHECK (single.binary[0] == "noArg\trefresh\t");
        // The typed views of the commands
        const std::vector<uint8_t> commands = dbm::messagesFromText ("int\ttrack\t3/mute\t1\nint\tbank\tfxtrack\t2\ndouble\tdevice\tparam/5/value/inc\t-0.5");
        single.processCommands (commands.data (), commands.size ());
        CHECK (single.fxBank == 2);
        int seen = 0;
        for (const dbm::protocol::Record &r : dbm::protocol::Reader (commands.data (), commands.size ()))
        {
            if (r.cmd () == dbm::protocol::Cmd::ChannelMute)
            {
                const auto c = r.as<dbm::protocol::cmd::ChannelMute> ();
                CHECK (c.ch_kind () == DBM_CHAN_KIND_TRACK && c.ch_processor () == DBM_PROCESSOR_TRACK && c.ch_index () == 3 && c.value ());
                seen++;
            }
            else if (r.cmd () == dbm::protocol::Cmd::ParamValue)
            {
                const auto c = r.as<dbm::protocol::cmd::ParamValue> ();
                CHECK (c.op () == DBM_OP_INC && c.value () == -0.5 && c.dev () == DBM_PROCESSOR_DEVICE && c.param () == 5);
                seen++;
            }
        }
        CHECK (seen == 2);

        dbm::protocol::Writer report;
        report.trackCount (4);
        report.channelExists (DBM_CH_KIND_TRACK, 0, true);
        report.channelName (DBM_CH_KIND_TRACK, 0, "Synth");
        report.channelColor (DBM_CH_KIND_TRACK, 0, dbm::protocol::Color{ 255, 0, 51, true });
        report.channelVolumeStr (DBM_CH_KIND_TRACK, 0, 8, "-6 dB");
        report.paramValue (DBM_PARAM_OWNER_DEVICE, 3, 0.25);
        CHECK (dbm::messagesToText (report.data (), report.size ()) == "/track/count 4\n/track/0/exists 1\n/track/0/name Synth\n/track/0/color 255 0 51\n/track/0/volume/str/8 -6 dB\n/device/param/3/value 0.25");
        const std::vector<uint8_t> converted = dbm::messagesFromText ("/track/count 4\n/track/0/exists 1\n/track/0/name Synth\n/track/0/color 255 0 51\n/track/0/volume/str/8 -6 dB\n/device/param/3/value 0.25");
        CHECK (converted == std::vector<uint8_t> (report.data (), report.data () + report.size ()));
        app.updateModelBin (report);
        app.tick ();
        CHECK (single.commands.empty ());

        // The reader of a C buffer
        uint8_t data[64];
        DbmWriter w;
        dbm_writer_init (&w, data, sizeof (data));
        CHECK (dbm_put_f64 (&w, DBM_MSG_TEMPO, 0, 0, 0, 0, 120.5) == 1);
        CHECK (dbm_put_text (&w, DBM_MSG_PROJECT_NAME, 0, 0, 0, 0, "Song", 4) == 1);
        CHECK (dbm_put_text (&w, DBM_MSG_PROJECT_NAME, 0, 0, 0, 0, "Too long for the buffer", 23) == 0);
        int records = 0;
        for (const dbm::protocol::Record &r : dbm::protocol::Reader (data, w.size))
        {
            records++;
            if (r.msg () == dbm::protocol::Msg::Tempo)
                CHECK (r.doubleValue () == 120.5);
            else
                CHECK (r.text () == "Song");
        }
        CHECK (records == 2);
        CHECK (dbm::messagesToText (data, w.size) == "/tempo 120.5\n/project/name Song");
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
