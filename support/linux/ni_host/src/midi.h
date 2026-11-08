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

    // The messages between DrivenByMoss and dbm-nihost (not in NI's driver): F0 7D 44 42 4D <id> ...
    //   10 00 <56 characters> F7  the labels of the Mk2's buttons above the displays (to dbm-nihost)
    //   11 F7                     the query whether the device is there (to dbm-nihost); the Mk2's
    //                             core asks with it whether the Shift button is sent
    //   11 01 F7                  the Mk2's hello: it is there and its Shift button is sent; also
    //                             sent when it is attached and the DAW is connected
    //   12 01|00 F7               the Mk2's Shift button down / up
    //   13 F7                     the device is gone (unplugged, dbm-nihost stops); the zynthian
    //                             drivers stop their controller until the next hello
    // The JAM answers the query with NI's "return from host" sysex, its hello.
    namespace dbm_sysex
    {
        constexpr uint8_t header[] = { 0xF0, 0x7D, 0x44, 0x42, 0x4D };
        constexpr uint8_t labels = 0x10, host = 0x11, shift = 0x12, gone = 0x13;

        // F0 7D 44 42 4D <id> F7, or with one value: F0 7D 44 42 4D <id> <value> F7
        inline void send( const MidiOut& out, uint8_t id, int value = -1 )
        {
            uint8_t message[sizeof header + 3];
            size_t  length = 0;
            for( const uint8_t byte : header )
                message[length++] = byte;
            message[length++] = id;
            if( value >= 0 )
                message[length++] = (uint8_t)( value & 0x7F );
            message[length++] = 0xF7;
            out( message, length );
        }

        // F0 7D 44 42 4D 11 F7: the query whether the device is there
        inline bool isQuery( const uint8_t* data, size_t length )
        {
            if( length != sizeof header + 2 || data[sizeof header] != host || data[sizeof header + 1] != 0xF7 )
                return false;
            for( size_t i = 0; i < sizeof header; i++ )
                if( data[i] != header[i] )
                    return false;
            return true;
        }
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
