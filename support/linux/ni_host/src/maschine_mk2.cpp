// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
//
// Taken from KICK's kick_controllers module (same author, relicensed from AGPL-3.0). The
// protocol tables come from ni-usb/cabl, (C) 2015 Vincenzo Pacella, MIT license (see
// LICENSE-cabl.txt).

#include "maschine_mk2.h"

#include <algorithm>
#include <cmath>
#include <cstring>

namespace dbm::nihost
{
    namespace
    {
        // pad pressure 0..0xFFF: on above, off below (cabl: 200 for both)
        constexpr int padOn = 200, padOff = 100;
        // samples after the pad went on that still count for its velocity
        // (a report carries two per pad)
        constexpr int attackSamples = 4;
        // the pressure that gives velocity 127
        constexpr int padFull = 3000;

        int velocity( int peak )
        {
            const double x = std::clamp( double( peak - padOn ) / ( padFull - padOn ), 0.0, 1.0 );
            return 1 + (int)std::lround( 126.0 * std::sqrt( x ) );
        }
    }

    void MaschineMk2::reset()
    {
        buttons.fill( 0 );
        mainPosition = -1;
        encoderPosition.fill( -1 );
        pads.fill( {} );
        padsChanged = groupsChanged = buttonsChanged = true;
        displayChunks.fill( 0xFF );
    }

    bool MaschineMk2::isDown( int button ) const
    {
        return button >= 0 && button < NumButtons && ( ( buttons[(size_t)button / 8] >> ( button % 8 ) ) & 1 );
    }

    void MaschineMk2::decode( const uint8_t* report, size_t length, Listener& listener )
    {
        if( length == 0 )
            return;

        // report 0x01: [1..7] buttons (LSB first), [8] main encoder (4 bit
        // counter, counts up clockwise, as in cabl's MaschineMK2.cpp), [9..24]
        // encoders 1..8 (u16 LE, 10 bit counters, count up clockwise)
        if( report[0] == 0x01 && length >= 25 )
        {
            for( size_t i = 0; i < buttons.size(); i++ )
            {
                const uint8_t now     = report[1 + i];
                uint8_t       changed = now ^ buttons[i];
                buttons[i]            = now;
                for( int bit = 0; changed; bit++, changed >>= 1 )
                    if( ( changed & 1 ) && (int)i * 8 + bit < NumButtons )
                        listener.mk2Button( (int)i * 8 + bit, ( now >> bit ) & 1 );
            }

            const int main = report[8] & 0x0F;
            if( mainPosition >= 0 && main != mainPosition )
            {
                int steps = ( main - mainPosition ) & 0x0F;
                if( steps >= 8 )
                    steps -= 16;
                listener.mk2MainEncoder( steps );
            }
            mainPosition = main;

            for( int e = 0; e < numEncoders; e++ )
            {
                const int position = ( report[9 + 2 * e] | ( report[10 + 2 * e] << 8 ) ) & 0x3FF;
                auto&     last     = encoderPosition[(size_t)e];
                if( last >= 0 && position != last )
                {
                    int delta = ( position - last ) & 0x3FF;
                    if( delta >= 512 )
                        delta -= 1024;
                    listener.mk2Encoder( e, delta );
                }
                last = position;
            }
        }

        // report 0x20: 32 x u16 LE, pad number in the top 4 bits, 12 bit pressure
        else if( report[0] == 0x20 )
        {
            for( size_t i = 1; i + 1 < length && i < 65; i += 2 )
            {
                const int word     = report[i] | ( report[i + 1] << 8 );
                const int pressure = word & 0x0FFF;
                auto&     p        = pads[(size_t)( word >> 12 )];
                const int pad      = word >> 12;

                if( p.pending )
                {
                    // the velocity is the peak of the first samples
                    p.peak = std::max( p.peak, pressure );
                    if( ++p.samples >= attackSamples || pressure < padOff )
                    {
                        p.pending = false;
                        listener.mk2Pad( pad, true, velocity( p.peak ) );
                        if( pressure < padOff )
                        {
                            p.down = false;
                            listener.mk2Pad( pad, false, 0 );
                        }
                    }
                }
                else if( !p.down && pressure > padOn )
                {
                    p.down    = true;
                    p.pending = true;
                    p.peak    = pressure;
                    p.samples = 1;
                }
                else if( p.down && pressure < padOff )
                {
                    p.down = false;
                    listener.mk2Pad( pad, false, 0 );
                }
            }
        }
    }

    void MaschineMk2::setLed( int led, uint8_t brightness )
    {
        if( led >= 0 && led < numButtonLeds )
        {
            if( buttonLeds[(size_t)led] != brightness )
            {
                buttonLeds[(size_t)led] = brightness;
                buttonsChanged          = true;
            }
        }
        else if( led >= LedRestart && led < NumLeds )
        {
            auto& v = groupLeds[(size_t)( 48 + led - LedRestart )];
            if( v != brightness )
            {
                v             = brightness;
                groupsChanged = true;
            }
        }
    }

    void MaschineMk2::setPad( int pad, uint8_t r, uint8_t g, uint8_t b )
    {
        if( pad < 0 || pad >= numPads )
            return;
        uint8_t* p = &padLeds[(size_t)pad * 3];
        if( p[0] != r || p[1] != g || p[2] != b )
        {
            p[0] = r, p[1] = g, p[2] = b;
            padsChanged = true;
        }
    }

    void MaschineMk2::setGroup( int group, uint8_t r, uint8_t g, uint8_t b )
    {
        if( group < 0 || group >= 8 )
            return;
        uint8_t* p = &groupLeds[(size_t)group * 6]; // two RGB LEDs per group
        if( p[0] != r || p[1] != g || p[2] != b || p[3] != r || p[4] != g || p[5] != b )
        {
            p[0] = p[3] = r, p[1] = p[4] = g, p[2] = p[5] = b;
            groupsChanged = true;
        }
    }

    void MaschineMk2::setDisplay( int display, const Display& pixels )
    {
        if( display < 0 || display > 1 )
            return;
        auto&               d     = displays[(size_t)display];
        constexpr size_t    chunk = displayBytes / 8;
        for( size_t c = 0; c < 8; c++ )
            if( std::memcmp( &d[c * chunk], &pixels[c * chunk], chunk ) != 0 )
            {
                std::memcpy( &d[c * chunk], &pixels[c * chunk], chunk );
                displayChunks[(size_t)display] |= (uint8_t)( 1 << c );
            }
    }

    void MaschineMk2::flush( const std::function<bool( const uint8_t*, size_t )>& write )
    {
        auto send = [&]( uint8_t id, const uint8_t* data, size_t length, bool& changed ) {
            if( !changed )
                return;
            uint8_t report[1 + 56];
            report[0] = id;
            std::memcpy( report + 1, data, length );
            changed = !write( report, 1 + length );
        };
        send( 0x80, padLeds.data(), padLeds.size(), padsChanged );
        send( 0x81, groupLeds.data(), groupLeds.size(), groupsChanged );
        send( 0x82, buttonLeds.data(), buttonLeds.size(), buttonsChanged );

        // displays 0xE0 / 0xE1: 8 reports of 8 rows: header 00 00 row 00, width
        // in bytes (u16 LE), rows (u16 LE), then 256 bytes
        for( int d = 0; d < 2; d++ )
            for( int c = 0; c < 8; c++ )
            {
                if( !( displayChunks[(size_t)d] & ( 1 << c ) ) )
                    continue;
                uint8_t report[9 + 256] = { (uint8_t)( 0xE0 | d ), 0x00, 0x00, (uint8_t)( c * 8 ), 0x00, 0x20, 0x00, 0x08, 0x00 };
                std::memcpy( report + 9, &displays[(size_t)d][(size_t)c * 256], 256 );
                if( write( report, sizeof report ) )
                    displayChunks[(size_t)d] &= (uint8_t) ~( 1 << c );
            }
    }
}
