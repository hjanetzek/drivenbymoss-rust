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
    // The USB HID protocol of a Native Instruments Maschine JAM (17cc:1500),
    // without any knowledge of the host. The JAM has no MIDI interface: its MIDI
    // mode is made by NI's driver on the PC, so it is driven as raw HID here.
    // Report layout, button and LED order and palette from ni-usb/cabl
    // (C) 2015 Vincenzo Pacella, MIT license, as checked on the device for
    // MicroDexed-touch's maschine_jam.cpp (the same author as this file).
    // Not thread safe: decode, the LED setters and flush belong to one thread.
    class MaschineJam
    {
    public:
        static constexpr int vendorId  = 0x17CC;
        static constexpr int productId = 0x1500;

        // bits of input report 0x01, from byte 2
        enum Button
        {
            Song,
            Scene1, // .. + 7, above the pads
            Step = Scene1 + 8,
            PadMode,
            Clear,
            Duplicate,
            Up,
            Left,
            Right,
            Down,
            NoteRepeat,
            Pad1, // .. + 63, row major from the top left
            GroupA = Pad1 + 64, // .. + 7, below the pads
            Master = GroupA + 8,
            Group,
            In,
            Cue,
            Browse,
            Macro,
            Level,
            Aux,
            Control,
            Auto,
            Perform,
            Variation,
            Lock,
            Tune,
            Swing,
            Shift,
            Play,
            Rec,
            PageLeft,
            PageRight,
            Tempo,
            Grid,
            Solo,
            Mute,
            Select,
            EncoderTouch,
            EncoderPush,
            NumButtons
        };

        // single colour LEDs (output report 0x80), brightness 0..0x7F
        enum MonoLed
        {
            LedSong,
            LedStep,
            LedPadMode,
            LedClear,
            LedDuplicate,
            LedUp,
            LedLeft,
            LedRight,
            LedDown,
            LedNoteRepeat,
            LedMaster,
            LedGroup,
            LedIn,
            LedCue = 14,
            LedBrowse = 16,
            LedMacro,
            LedLevel,
            LedAux,
            LedControl,
            LedAuto,
            LedPerform,
            LedVariation,
            LedLock,
            LedTune,
            LedSwing,
            LedShift,
            LedPlay,
            LedRec,
            LedPageLeft,
            LedPageRight,
            LedTempo,
            LedGrid,
            LedSolo,
            LedMute,
            LedSelect,
            LedLevelLeft1, // .. + 7, bottom to top
            LedLevelRight1 = LedLevelLeft1 + 8,
            NumMonoLeds    = LedLevelRight1 + 8,
            NoLed          = -1
        };

        static constexpr int numPads       = 64;
        static constexpr int numStrips     = 8;
        static constexpr int stripLeds     = 11;
        static constexpr int stripMax      = 0x3FF;
        static constexpr int numRgbLeds    = 80; // report 0x81: 0..7 scenes, 8..71 pads, 72..79 groups
        static constexpr int numButtonBytes = 15;

        static constexpr uint8_t lightOff = 0, lightDim = 0x0C, lightOn = 0x7F;

        // palette (reports 0x81, 0x82): hue * 4 + level
        enum Hue
        {
            HueOff    = 0,
            HueRed    = 1,
            HueOrange = 2,
            HueAmber  = 3,
            HueYellow = 4,
            HueLime   = 5,
            HueGreen  = 7,
            HueCyan   = 9,
            HueSky    = 10,
            HueBlue   = 11,
            HueViolet = 12,
            HuePurple = 13,
            HueMagenta = 14,
            HuePink   = 15,
            HueWhite  = 17
        };
        enum Level
        {
            LevelDim,
            LevelMedium,
            LevelFull,
            LevelPale
        };
        static uint8_t colour( int hue, int level ) { return hue == HueOff ? 0 : (uint8_t)( hue * 4 + ( level & 3 ) ); }

        struct Listener
        {
            virtual ~Listener() = default;
            virtual void jamButton( int button, bool down ) = 0;
            virtual void jamEncoder( int steps )            = 0;
            // position 0..stripMax from the bottom, -1: released
            virtual void jamStrip( int strip, int position ) = 0;
        };

        // A device was attached: forget the input state and send all LEDs
        // (off unless set) with the next flush.
        void reset();
        // an input report (report[0] = report ID)
        void decode( const uint8_t* report, size_t length, Listener& listener );
        bool isDown( int button ) const;

        void setMono( int led, uint8_t brightness );
        void setScene( int scene, uint8_t colour );
        void setPad( int x, int y, uint8_t colour ); // y = 0: the top row
        void setGroup( int group, uint8_t colour );
        void setStripBar( int strip, int leds, uint8_t colour ); // leds from the bottom
        // A single strip LED (0 = bottom); added for the NI host emulation
        void setStripLed( int strip, int led, uint8_t colour );

        // Sends the reports that changed; a report that fails is sent again
        // with the next flush.
        void flush( const std::function<bool( const uint8_t*, size_t )>& write );

    private:
        void setRgb( int index, uint8_t value );

        std::array<uint8_t, numButtonBytes> buttons{};
        int                                 encoderPosition{ -1 }; // 4 bit counter
        std::array<int, numStrips>          stripPosition{};

        std::array<uint8_t, NumMonoLeds>           mono{};
        std::array<uint8_t, numRgbLeds>            rgb{};
        std::array<uint8_t, numStrips * stripLeds> strips{};
        bool                                       monoChanged{ true }, rgbChanged{ true }, stripsChanged{ true };
    };
}
