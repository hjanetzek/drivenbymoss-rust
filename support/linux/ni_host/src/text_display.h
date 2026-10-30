// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <string>

namespace dbm::nihost
{
    // The text of a Mackie MCU display (2 lines of 56 characters), as NI's driver shows it on
    // the two graphic displays of the Maschine Mk2: each display shows 28 characters of both
    // lines. Additionally the labels of the 8 buttons above the displays (56 characters, sent by
    // DrivenByMoss: F0 7D 44 42 4D 10 <position> <characters> F7), shown above the lines.
    class McuTextDisplay
    {
    public:
        static constexpr int    lineLength = 56, lines = 2;
        static constexpr size_t displayWidth = 256, displayHeight = 64;
        using Bitmap = std::array<uint8_t, displayWidth / 8 * displayHeight>; // 1 bit per pixel, MSB left

        McuTextDisplay();

        // Handle a sysex message; true if it was an MCU display update (F0 00 00 66 14 12 ...).
        bool handleSysex( const uint8_t* data, size_t length );

        void        setText( int position, const std::string& text );
        std::string line( int index ) const;
        std::string labels() const;

        // Render the part of the text which is shown on the display (0 = left, 1 = right).
        Bitmap render( int display ) const;

        // True if the text changed since the last call.
        bool takeChanged();

    private:
        std::array<char, lineLength * lines> text{};
        std::array<char, lineLength>         buttonLabels{};
        bool                                 changed{ true };
    };
}
