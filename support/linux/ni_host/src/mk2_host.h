// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#pragma once

#include "maschine_mk2.h"
#include "midi.h"
#include "text_display.h"

#include <array>
#include <chrono>
#include <map>

namespace dbm::nihost
{
    // Emulates NI's driver for the Maschine Mk2 in MIDI mode with the DrivenByMoss template:
    // HID input becomes the template's MIDI, MIDI from the DAW sets the LEDs and the MCU text
    // of the two displays.
    // Not thread safe: all functions belong to one thread (or are called under one lock).
    class Mk2Host : private MaschineMk2::Listener
    {
    public:
        explicit Mk2Host( MidiOut out );

        // The device was attached: resend all LEDs and the displays, and announce that the Shift
        // button is sent (the hello, see announceShift).
        void attached();
        // The device is gone (unplugged, or dbm-nihost stops): tell the DAW (F0 7D 44 42 4D 13 F7).
        void detached();
        // The ports were connected to the DAW (snd-virmidi): announce that the Shift button is
        // sent, for a DAW which was started before dbm-nihost (only while the device is there).
        void dawConnected();
        void hidReport( const uint8_t* report, size_t length );
        // A MIDI message from the DAW (short message or complete sysex).
        void midiIn( const uint8_t* data, size_t length );
        // Send the changed LED reports and, at most 20 times a second, the displays.
        void flush( const ReportWriter& write );

        const McuTextDisplay& display() const { return this->text; }

        // Counts of the 10 bit encoders per MIDI step: the template has 100 steps per turn.
        static constexpr int encoderCountsPerTurn = 1024, stepsPerTurn = 100;

    private:
        void mk2Button( int button, bool down ) override;
        void mk2MainEncoder( int steps ) override;
        void mk2Encoder( int encoder, int delta ) override;
        void mk2Pad( int pad, bool down, int velocity ) override;

        void setButtonLed( int button, int value );
        // Tell DrivenByMoss that the device is there and the Shift button is sent (the hello,
        // F0 7D 44 42 4D 11 01 F7), it no longer emulates Shift with STEP LEFT then; also the
        // answer to its query (F0 7D 44 42 4D 11 F7) while the device is there.
        void announceShift();

        MaschineMk2    mk2;
        MidiOut        out;
        McuTextDisplay text;

        std::array<const TemplateControl*, MaschineMk2::NumButtons>  buttonControls{};
        std::array<const TemplateControl*, MaschineMk2::numEncoders> knobs{};
        std::array<const TemplateControl*, MaschineMk2::numPads>     pads{}; // by the index of MaschineMk2
        const TemplateControl*                                       dial{ nullptr };
        // CC -> button for the LED feedback, note -> pad index
        std::map<int, int> ccButtons, notePads;

        bool                                      present{ false }; // the device is attached
        std::array<int, MaschineMk2::numEncoders> encoderRemainder{};
        std::chrono::steady_clock::time_point     lastDisplayUpdate{};
    };
}
