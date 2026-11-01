// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
//
// Tests of the translation between HID reports and the template MIDI, without hardware.

#include "jam_host.h"
#include "mk2_host.h"
#include "palette.h"

#include <chrono>
#include <cstdio>
#include <thread>
#include <string>
#include <vector>

using namespace dbm::nihost;

static int failures = 0;

#define CHECK( condition )                                                                \
    do                                                                                    \
    {                                                                                     \
        if( !( condition ) )                                                              \
        {                                                                                 \
            std::fprintf( stderr, "FAILED: %s (line %d)\n", #condition, __LINE__ );       \
            failures++;                                                                   \
        }                                                                                 \
    } while( 0 )

using Bytes = std::vector<uint8_t>;

struct Recorder
{
    std::vector<Bytes> midi;
    MidiOut            out() { return [this]( const uint8_t* d, size_t n ) { this->midi.emplace_back( d, d + n ); }; }
    bool               has( const Bytes& message ) const
    {
        for( const Bytes& m : this->midi )
            if( m == message )
                return true;
        return false;
    }
};

struct Reports
{
    std::vector<Bytes> reports;
    ReportWriter       writer() { return [this]( const uint8_t* d, size_t n ) { this->reports.emplace_back( d, d + n ); return true; }; }
    const Bytes*       find( uint8_t id ) const
    {
        for( auto it = this->reports.rbegin(); it != this->reports.rend(); ++it )
            if( !it->empty() && ( *it )[0] == id )
                return &*it;
        return nullptr;
    }
};

static void setBit( Bytes& report, size_t firstByte, int bit )
{
    report[firstByte + (size_t)( bit / 8 )] |= (uint8_t)( 1 << ( bit % 8 ) );
}

static void testJam()
{
    Recorder midi;
    JamHost  host( midi.out() );
    host.attached();
    // the "return from host" sysex
    CHECK( midi.has( { 0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x46, 0x01, 0xF7 } ) );

    // buttons: report 1, bits from byte 2
    Bytes report( 2 + MaschineJam::numButtonBytes, 0 );
    report[0] = 0x01;
    report[1] = 3; // encoder position
    host.hidReport( report.data(), report.size() );
    midi.midi.clear();

    setBit( report, 2, MaschineJam::Play );
    setBit( report, 2, MaschineJam::Pad1 );       // top left
    setBit( report, 2, MaschineJam::Pad1 + 63 );  // bottom right
    setBit( report, 2, MaschineJam::Shift );
    report[1] = 4; // encoder one step clockwise
    host.hidReport( report.data(), report.size() );
    CHECK( midi.has( { 0xB0, 90, 127 } ) );
    CHECK( midi.has( { 0x90, 92, 127 } ) );
    CHECK( midi.has( { 0x90, 43, 127 } ) );
    CHECK( midi.has( { 0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x4D, 0x01, 0xF7 } ) );
    CHECK( midi.has( { 0xB0, 57, 1 } ) );

    std::fill( report.begin() + 2, report.end(), 0 );
    report[1] = 2; // two steps counter clockwise
    midi.midi.clear();
    host.hidReport( report.data(), report.size() );
    CHECK( midi.has( { 0xB0, 90, 0 } ) );
    CHECK( midi.has( { 0x80, 92, 0 } ) );
    CHECK( midi.has( { 0xB0, 57, 126 } ) );

    // touch strips: report 2, per strip time, finger 1, finger 2 (u16 LE)
    Bytes strips( 1 + MaschineJam::numStrips * 6, 0 );
    strips[0] = 0x02;
    strips[3] = 0x00;
    strips[4] = 0x02; // strip 1 at 512
    midi.midi.clear();
    host.hidReport( strips.data(), strips.size() );
    CHECK( midi.has( { 0xB0, 20, 127 } ) );
    CHECK( midi.has( { 0xB0, 8, 512 * 127 / MaschineJam::stripMax } ) );
    strips[4] = 0;
    midi.midi.clear();
    host.hidReport( strips.data(), strips.size() );
    CHECK( midi.has( { 0xB0, 20, 0 } ) );

    // LEDs from the DAW
    Reports out;
    host.flush( out.writer() );
    out.reports.clear();
    const uint8_t pad[] = { 0x90, 92, 6 }, group[] = { 0xB0, 70, 78 }, play[] = { 0xB0, 90, 127 }, rec[] = { 0xB0, 91, 0x30 };
    host.midiIn( pad, 3 );
    host.midiIn( group, 3 );
    host.midiIn( play, 3 );
    host.midiIn( rec, 3 );
    host.flush( out.writer() );
    const Bytes* rgb = out.find( 0x81 );
    CHECK( rgb != nullptr && ( *rgb )[1 + 8] == 6 );       // pad top left
    CHECK( rgb != nullptr && ( *rgb )[1 + 72] == 78 );     // group A
    const Bytes* mono = out.find( 0x80 );
    CHECK( mono != nullptr && ( *mono )[1 + MaschineJam::LedPlay] == 0x7F );
    CHECK( mono != nullptr && ( *mono )[1 + MaschineJam::LedRec] == 0x30 * 0x30 / 0x7F );

    // strip setup: single mode, value 127 -> all 11 LEDs in the strip colour
    const uint8_t setup[] = { 0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x05, 0, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xF7 };
    const uint8_t value[] = { 0xB0, 8, 127 };
    host.midiIn( setup, sizeof setup );
    host.midiIn( value, 3 );
    out.reports.clear();
    host.flush( out.writer() );
    const Bytes* strip = out.find( 0x82 );
    CHECK( strip != nullptr && ( *strip )[1] == 30 && ( *strip )[11] == 30 && ( *strip )[12] == 0 );
}

static void testMk2()
{
    Recorder    midi;
    Mk2Host     host( midi.out() );
    host.attached();
    // the announcement that the Shift button is sent (DrivenByMoss)
    const Bytes sendsShift = { 0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x11, 0x01, 0xF7 };
    CHECK( midi.has( sendsShift ) );
    midi.midi.clear();
    host.dawConnected();
    CHECK( midi.midi.size() == 1 && midi.has( sendsShift ) );
    midi.midi.clear();
    // the answer to the query of DrivenByMoss
    const uint8_t query[] = { 0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x11, 0xF7 };
    host.midiIn( query, sizeof query );
    CHECK( midi.midi.size() == 1 && midi.has( sendsShift ) );

    Bytes report( 25, 0 );
    report[0] = 0x01;
    host.hidReport( report.data(), report.size() );
    midi.midi.clear();

    // Shift is sent as a DrivenByMoss sysex (it is not in the template)
    setBit( report, 1, MaschineMk2::Shift );
    host.hidReport( report.data(), report.size() );
    CHECK( midi.midi.size() == 1 && midi.has( { 0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x12, 0x01, 0xF7 } ) );
    report.assign( report.size(), 0 );
    report[0] = 0x01;
    host.hidReport( report.data(), report.size() );
    CHECK( midi.midi.size() == 2 && midi.has( { 0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x12, 0x00, 0xF7 } ) );
    midi.midi.clear();

    setBit( report, 1, MaschineMk2::Play );
    setBit( report, 1, MaschineMk2::Display1 + 6 ); // the MCU button
    report[9] = 11; // encoder 1: 11 counts = 1 step
    host.hidReport( report.data(), report.size() );
    CHECK( midi.has( { 0xB0, 57, 127 } ) );
    CHECK( midi.has( { 0x90, 28, 127 } ) );
    CHECK( midi.has( { 0xB0, 70, 1 } ) );

    // pads: report 0x20, pad number in the top 4 bits; MaschineMk2 pad 0 = top left = NI pad 13
    Bytes pads( 65, 0 );
    pads[0] = 0x20;
    const auto sample = [&]( int pad, int pressure ) {
        const int word = ( pad << 12 ) | pressure;
        pads[1]        = (uint8_t)( word & 0xFF );
        pads[2]        = (uint8_t)( word >> 8 );
        host.hidReport( pads.data(), pads.size() );
    };
    midi.midi.clear();
    for( int i = 0; i < 4; i++ )
        sample( 0, 3000 );
    sample( 0, 0 );
    bool noteOn = false;
    for( const Bytes& m : midi.midi )
        if( m.size() == 3 && m[0] == 0x90 && m[1] == 48 && m[2] > 0 )
            noteOn = true;
    CHECK( noteOn );
    CHECK( midi.has( { 0x80, 48, 0 } ) );

    // LEDs and display from the DAW
    Reports out;
    host.flush( out.writer() );
    out.reports.clear();
    const uint8_t padColour[] = { 0x90, 48, 6 }, play[] = { 0xB0, 57, 127 };
    host.midiIn( padColour, 3 );
    host.midiIn( play, 3 );
    const std::string text = "Hello";
    Bytes             sysex = { 0xF0, 0x00, 0x00, 0x66, 0x14, 0x12, 0x00 };
    sysex.insert( sysex.end(), text.begin(), text.end() );
    sysex.push_back( 0xF7 );
    host.midiIn( sysex.data(), sysex.size() );
    CHECK( host.display().line( 0 ).rfind( "Hello", 0 ) == 0 );
    // the labels of the buttons above the displays (DrivenByMoss), drawn above the lines
    const std::string labels = ">Volume Pan  ";
    Bytes             labelsSysex = { 0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x10, 0x00 };
    labelsSysex.insert( labelsSysex.end(), labels.begin(), labels.end() );
    labelsSysex.push_back( 0xF7 );
    host.midiIn( labelsSysex.data(), labelsSysex.size() );
    CHECK( host.display().labels().rfind( ">Volume Pan", 0 ) == 0 );
    CHECK( host.display().line( 0 ).rfind( "Hello", 0 ) == 0 );
    {
        const auto bitmap = host.display().render( 0 );
        bool       hasLabelPixels = false;
        for( size_t i = 0; i < 12 * dbm::nihost::McuTextDisplay::displayWidth / 8; i++ )
            hasLabelPixels |= bitmap[i] != 0;
        CHECK( hasLabelPixels );
    }
    // the displays are updated at most 20 times a second
    std::this_thread::sleep_for( std::chrono::milliseconds( 60 ) );
    host.flush( out.writer() );
    const Bytes* padReport = out.find( 0x80 );
    CHECK( padReport != nullptr && ( *padReport )[1] == 255 / 2 && ( *padReport )[2] == 0 ); // red, top left
    CHECK( out.find( 0xE0 ) != nullptr ); // a display chunk
}

int main()
{
    CHECK( paletteColour( 78 ).r > 200 );
    CHECK( paletteColour( 200 ).r == 0 );
    testJam();
    testMk2();
    if( failures == 0 )
        std::printf( "All ni_host tests passed.\n" );
    return failures == 0 ? 0 : 1;
}
