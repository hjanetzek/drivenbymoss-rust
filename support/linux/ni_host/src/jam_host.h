// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#pragma once

#include "maschine_jam.h"
#include "midi.h"

#include <array>
#include <map>

namespace dbm::nihost
{
    // Emulates NI's driver for the Maschine JAM in MIDI mode with the DrivenByMoss template:
    // HID input becomes the template's MIDI, MIDI from the DAW sets the LEDs.
    // Not thread safe: all functions belong to one thread (or are called under one lock).
    class JamHost : private MaschineJam::Listener
    {
    public:
        explicit JamHost( MidiOut out );

        // The device was attached: resend all LEDs and tell the DAW (NI's "return from host"
        // sysex), which then refreshes its state.
        void attached();
        // The ports were connected to the DAW (snd-virmidi): nothing to announce, NI's driver
        // sends the Shift button of the JAM itself.
        void dawConnected() {}
        // An input report of the device.
        void hidReport( const uint8_t* report, size_t length );
        // A MIDI message from the DAW (short message or complete sysex).
        void midiIn( const uint8_t* data, size_t length );
        // Send the changed LED reports.
        void flush( const ReportWriter& write );

        // The strip modes of the STRIP_SETUP sysex.
        enum StripType
        {
            StripSingle = 0,
            StripDot    = 1,
            StripPan    = 2,
            StripDual   = 3
        };

    private:
        void jamButton( int button, bool down ) override;
        void jamEncoder( int steps ) override;
        void jamStrip( int strip, int position ) override;

        void setButtonLed( int button, int value );
        void drawStrip( int strip );
        void handleSysex( const uint8_t* data, size_t length );

        MaschineJam jam;
        MidiOut     out;

        // template controls of the HID buttons (nullptr: not in the template)
        std::array<const TemplateControl*, MaschineJam::NumButtons> buttonControls{};
        const TemplateControl*                                      encoder{ nullptr };
        std::array<const TemplateControl*, MaschineJam::numStrips>  stripValue{}, stripTouch{};
        // CC -> button for the LED feedback of the mono LED buttons, the groups and scenes
        std::map<int, int> ccButtons;
        // note -> pad index (row major from the top left)
        std::map<int, int> notePads;

        struct Strip
        {
            int type{ StripSingle }, colour{ 0 }, value{ 0 }, dualValue{ 0 };
        };
        std::array<Strip, MaschineJam::numStrips> strips{};
        std::array<bool, MaschineJam::numStrips>  stripTouched{};
    };
}
