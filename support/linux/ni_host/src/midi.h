// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#pragma once

#include "template.h"

#include <cstddef>
#include <cstdint>
#include <functional>

namespace dbm::nihost
{
    // Receives the MIDI which the emulated NI driver sends to the DAW (one message per call).
    using MidiOut = std::function<void( const uint8_t* data, size_t length )>;

    // Writes an output report to the device; false if it failed.
    using ReportWriter = std::function<bool( const uint8_t* report, size_t length )>;

    // Send the message of a template control with a value.
    inline void sendControl( const MidiOut& out, const TemplateControl& control, int value )
    {
        uint8_t status = 0;
        switch( control.kind )
        {
            case MessageKind::Cc:
                status = 0xB0;
                break;
            case MessageKind::Note:
                status = value > 0 ? 0x90 : 0x80;
                break;
            case MessageKind::PolyAt:
                status = 0xA0;
                break;
        }
        const uint8_t message[3] = { (uint8_t)( status | ( control.channel & 0x0F ) ), (uint8_t)( control.number & 0x7F ),
                                     (uint8_t)( value < 0 ? 0 : ( value > 127 ? 127 : value ) ) };
        out( message, 3 );
    }

    // A relative value in two's complement (1 = +1, 127 = -1), as DrivenByMoss decodes it.
    inline int twosComplement( int steps )
    {
        if( steps > 63 )
            steps = 63;
        if( steps < -63 )
            steps = -63;
        return steps < 0 ? steps + 128 : steps;
    }
}
