// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#pragma once

#include <cstdint>

namespace dbm::nihost
{
    struct Rgb
    {
        uint8_t r, g, b;
    };

    // The RGB colour (0..255 each) of an index of NI's colour palette (hue * 4 + level), as the
    // DAW sends it in MIDI mode. 0..71 from cabl's JAM palette; 72..79 are not in cabl and are
    // greys to white (DrivenByMoss: 76 dark grey, 77 grey, 78 white). Checked on the Mk2: 72..79
    // show white from dim to bright.
    Rgb paletteColour( int index );
}
