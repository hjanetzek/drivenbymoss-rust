// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
//
// Taken from KICK's kick_controllers module (same author, relicensed from AGPL-3.0). The
// protocol tables come from ni-usb/cabl, (C) 2015 Vincenzo Pacella, MIT license (see
// LICENSE-cabl.txt).

#include "maschine_jam.h"

#include <cstring>

namespace dbm::nihost
{
    void MaschineJam::reset()
    {
        buttons.fill( 0 );
        stripPosition.fill( 0 );
        encoderPosition = -1;
        monoChanged = rgbChanged = stripsChanged = true;
    }

    bool MaschineJam::isDown( int button ) const
    {
        return button >= 0 && button < NumButtons && ( ( buttons[(size_t)button / 8] >> ( button % 8 ) ) & 1 );
    }

    void MaschineJam::decode( const uint8_t* report, size_t length, Listener& listener )
    {
        if( length == 0 )
            return;

        // report 1: [1] encoder (low nibble), [2..16] button bits (LSB first)
        if( report[0] == 0x01 && length >= 2 + numButtonBytes )
        {
            // every change of the report, not just the first one
            for( int i = 0; i < numButtonBytes; i++ )
            {
                const uint8_t now     = report[2 + i];
                uint8_t       changed = now ^ buttons[(size_t)i];
                buttons[(size_t)i]    = now;
                for( int bit = 0; changed; bit++, changed >>= 1 )
                    if( ( changed & 1 ) && i * 8 + bit < NumButtons )
                        listener.jamButton( i * 8 + bit, ( now >> bit ) & 1 );
            }

            const int position = report[1] & 0x0F;
            if( encoderPosition >= 0 && position != encoderPosition )
            {
                int steps = ( position - encoderPosition ) & 0x0F;
                if( steps >= 8 )
                    steps -= 16;
                listener.jamEncoder( steps );
            }
            encoderPosition = position;
        }

        // report 2: per strip time, finger 1, finger 2 (u16 LE); 0 = not touched
        else if( report[0] == 0x02 && length >= 1 + numStrips * 6 )
        {
            for( int i = 0; i < numStrips; i++ )
            {
                const uint8_t* p        = report + 1 + i * 6;
                const int      position = p[2] | ( p[3] << 8 );
                if( position != stripPosition[(size_t)i] )
                {
                    stripPosition[(size_t)i] = position;
                    listener.jamStrip( i, position == 0 ? -1 : ( position > stripMax ? stripMax : position ) );
                }
            }
        }
    }

    void MaschineJam::setMono( int led, uint8_t brightness )
    {
        if( led >= 0 && led < NumMonoLeds && mono[(size_t)led] != brightness )
        {
            mono[(size_t)led] = brightness;
            monoChanged       = true;
        }
    }

    void MaschineJam::setRgb( int index, uint8_t value )
    {
        if( index >= 0 && index < numRgbLeds && rgb[(size_t)index] != value )
        {
            rgb[(size_t)index] = value;
            rgbChanged         = true;
        }
    }

    void MaschineJam::setScene( int scene, uint8_t c )
    {
        if( scene >= 0 && scene < 8 )
            setRgb( scene, c );
    }

    void MaschineJam::setPad( int x, int y, uint8_t c )
    {
        if( x >= 0 && x < 8 && y >= 0 && y < 8 )
            setRgb( 8 + y * 8 + x, c );
    }

    void MaschineJam::setGroup( int group, uint8_t c )
    {
        if( group >= 0 && group < 8 )
            setRgb( 72 + group, c );
    }

    void MaschineJam::setStripLed( int strip, int index, uint8_t c )
    {
        if( strip < 0 || strip >= numStrips || index < 0 || index >= stripLeds )
            return;
        auto& led = strips[(size_t)( strip * stripLeds + index )];
        if( led != c )
        {
            led           = c;
            stripsChanged = true;
        }
    }

    void MaschineJam::setStripBar( int strip, int leds, uint8_t c )
    {
        if( strip < 0 || strip >= numStrips )
            return;
        for( int i = 0; i < stripLeds; i++ )
        {
            auto&         led = strips[(size_t)( strip * stripLeds + i )];
            const uint8_t v   = i < leds ? c : 0;
            if( led != v )
            {
                led           = v;
                stripsChanged = true;
            }
        }
    }

    void MaschineJam::flush( const std::function<bool( const uint8_t*, size_t )>& write )
    {
        auto send = [&]( uint8_t id, const uint8_t* data, size_t length, bool& changed ) {
            if( !changed )
                return;
            uint8_t report[1 + numStrips * stripLeds];
            report[0] = id;
            std::memcpy( report + 1, data, length );
            changed = !write( report, 1 + length );
        };
        send( 0x80, mono.data(), mono.size(), monoChanged );
        send( 0x81, rgb.data(), rgb.size(), rgbChanged );
        send( 0x82, strips.data(), strips.size(), stripsChanged );
    }
}
