// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
//
// Taken from KICK's kick_controllers module (same author, relicensed from AGPL-3.0). The
// protocol tables come from ni-usb/cabl, (C) 2015 Vincenzo Pacella, MIT license (see
// LICENSE-cabl.txt).

#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <functional>

namespace dbm::nihost
{
    // The USB HID protocol of a Native Instruments Maschine Mk2 (17cc:1140),
    // without any knowledge of the host. Everything, the two displays included,
    // goes through the HID interface (output reports 0xE0 / 0xE1), so hidraw
    // is enough. Report layout, button and LED order and the display framing
    // from ni-usb/cabl's MaschineMK2 (C) 2015 Vincenzo Pacella, MIT license;
    // checked on the device (tools/mk2_probe.py), which also gave the encoder
    // directions and the pad order. Unlike cabl's class it handles every
    // change of a report and every pad sample.
    // Not thread safe: decode, the LED and display setters and flush belong to one thread.
    class MaschineMk2
    {
    public:
        static constexpr int vendorId  = 0x17CC;
        static constexpr int productId = 0x1140;

        // bits of input report 0x01, from byte 1
        enum Button
        {
            Display1, // .. + 7, above the displays
            Control = Display1 + 8,
            Step,
            Browse,
            Sampling,
            BrowseLeft,
            BrowseRight,
            All,
            AutoWrite,
            Volume,
            Swing,
            Tempo,
            MasterLeft,
            MasterRight,
            Enter,
            NoteRepeat,
            Main,
            GroupA, // .. + 7
            Restart = GroupA + 8,
            TransportLeft,
            TransportRight,
            Grid,
            Play,
            Rec,
            Erase,
            Shift,
            Scene,
            Pattern,
            PadMode,
            Navigate,
            Duplicate,
            Select,
            Solo,
            Mute,
            NumButtons
        };

        // single colour LEDs: 0..30 in output report 0x82, then the transport
        // LEDs at the end of report 0x81
        enum Led
        {
            LedControl,
            LedStep,
            LedBrowse,
            LedSampling,
            LedBrowseLeft,
            LedBrowseRight,
            LedAll,
            LedAutoWrite,
            LedDisplay1, // .. + 7
            LedScene = LedDisplay1 + 8,
            LedPattern,
            LedPadMode,
            LedNavigate,
            LedDuplicate,
            LedSelect,
            LedSolo,
            LedMute,
            LedVolume,
            LedSwing,
            LedTempo,
            LedMasterLeft,
            LedMasterRight,
            LedEnter,
            LedNoteRepeat,
            numButtonLeds,
            LedRestart = numButtonLeds, // report 0x81
            LedTransportLeft,
            LedTransportRight,
            LedGrid,
            LedPlay,
            LedRec,
            LedErase,
            LedShift,
            NumLeds,
            NoLed = -1
        };

        static constexpr int numPads        = 16; // index 0 top left, row major
        static constexpr int numEncoders    = 8;
        static constexpr int displayWidth   = 256;
        static constexpr int displayHeight  = 64;
        static constexpr int displayBytes   = displayWidth / 8 * displayHeight; // 1 bit per pixel, MSB left
        static constexpr uint8_t lightOff = 0, lightDim = 0x10, lightOn = 0x7F;

        using Display = std::array<uint8_t, displayBytes>;

        struct Listener
        {
            virtual ~Listener() = default;
            virtual void mk2Button( int button, bool down )             = 0;
            virtual void mk2MainEncoder( int steps )                     = 0; // > 0: clockwise
            virtual void mk2Encoder( int encoder, int delta )            = 0; // 10 bit counts, > 0: clockwise
            virtual void mk2Pad( int pad, bool down, int velocity )      = 0; // velocity 1..127
        };

        // A device was attached: forget the input state and send all LEDs and
        // both displays (dark unless set) with the next flush.
        void reset();
        void decode( const uint8_t* report, size_t length, Listener& listener );
        bool isDown( int button ) const;

        void setLed( int led, uint8_t brightness );
        void setPad( int pad, uint8_t r, uint8_t g, uint8_t b ); // 0..0x7F each
        void setGroup( int group, uint8_t r, uint8_t g, uint8_t b );
        void setDisplay( int display, const Display& pixels );

        // Sends the reports that changed; what fails is sent again with the next flush.
        void flush( const std::function<bool( const uint8_t*, size_t )>& write );

    private:
        std::array<uint8_t, 7>           buttons{};
        int                              mainPosition{ -1 }; // 4 bit counter
        std::array<int, numEncoders>     encoderPosition{};  // 10 bit counters, -1: unknown
        struct Pad
        {
            bool down{ false }, pending{ false };
            int  peak{ 0 }, samples{ 0 };
        };
        std::array<Pad, numPads> pads{};

        std::array<uint8_t, 48>            padLeds{};    // report 0x80
        std::array<uint8_t, 56>            groupLeds{};  // report 0x81: 8 x 2 x RGB, then the transport LEDs
        std::array<uint8_t, numButtonLeds> buttonLeds{}; // report 0x82
        std::array<Display, 2>             displays{};
        bool                               padsChanged{ true }, groupsChanged{ true }, buttonsChanged{ true };
        std::array<uint8_t, 2>             displayChunks{ { 0xFF, 0xFF } }; // the 8-row chunks to send
    };
}
