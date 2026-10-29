// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#pragma once

#include <cstddef>
#include <cstring>

namespace dbm::nihost
{
    enum class Direction
    {
        Input, // a control which sends MIDI to the DAW
        Led    // an LED which is set by MIDI from the DAW
    };

    enum class MessageKind
    {
        Cc,
        Note,
        PolyAt
    };

    // A control of an NI Controller Editor template (see tools/gen_template_map.py).
    struct TemplateControl
    {
        const char* id;
        Direction   direction;
        MessageKind kind;
        int         channel; // 0..15
        int         number;  // CC or note number
        bool        relative; // knob in relative ("comp") mode
        int         on, off;  // values of buttons
    };

    // Find a control by its template id, nullptr if the template has none.
    template <size_t N>
    const TemplateControl* findControl( const TemplateControl ( &controls )[N], const char* id, Direction direction )
    {
        for( const TemplateControl& control : controls )
            if( control.direction == direction && std::strcmp( control.id, id ) == 0 )
                return &control;
        return nullptr;
    }
}
