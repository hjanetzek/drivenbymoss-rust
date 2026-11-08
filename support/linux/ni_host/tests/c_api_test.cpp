// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
//
// Tests of libdbm_nihost's C interface with test devices (no hardware): synthetic HID reports
// become the template MIDI, MIDI from the DAW becomes LED and display reports.

#include "dbm_nihost.h"

// Only for the constants of the HID reports
#include "maschine_jam.h"
#include "maschine_mk2.h"

#include <chrono>
#include <cstdio>
#include <cstring>
#include <thread>
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

static const Bytes query          = { 0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x11, 0xF7 };
static const Bytes gone           = { 0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x13, 0xF7 };
static const Bytes mk2Hello       = { 0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x11, 0x01, 0xF7 };
static const Bytes jamHello       = { 0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01, 0x46, 0x01, 0xF7 };

// All messages waiting (no wait), split at the length prefixes
static std::vector<Bytes> readAll( dbm_nihost* device, int timeoutMs = 0 )
{
    std::vector<Bytes> messages;
    uint8_t            buffer[1024];
    while( true )
    {
        const int length = dbm_nihost_read( device, buffer, sizeof buffer, timeoutMs );
        if( length <= 0 )
            break;
        timeoutMs = 0;
        for( int i = 0; i + 2 <= length; )
        {
            const int size = buffer[i] | ( buffer[i + 1] << 8 );
            messages.emplace_back( buffer + i + 2, buffer + i + 2 + size );
            i += 2 + size;
        }
    }
    return messages;
}

static bool has( const std::vector<Bytes>& messages, const Bytes& message )
{
    for( const Bytes& m : messages )
        if( m == message )
            return true;
    return false;
}

static void send( dbm_nihost* device, const Bytes& message )
{
    CHECK( dbm_nihost_send( device, message.data(), message.size() ) == 0 );
}

// The reports written by the last flushes
static std::vector<Bytes> takeReports( dbm_nihost* device )
{
    std::vector<Bytes> reports;
    uint8_t            buffer[2048];
    int                length;
    while( ( length = dbm_nihost_test_take_report( device, buffer, sizeof buffer ) ) > 0 )
        reports.emplace_back( buffer, buffer + length );
    return reports;
}

static const Bytes* findReport( const std::vector<Bytes>& reports, uint8_t id )
{
    for( auto it = reports.rbegin(); it != reports.rend(); ++it )
        if( !it->empty() && ( *it )[0] == id )
            return &*it;
    return nullptr;
}

static void setBit( Bytes& report, size_t firstByte, int bit )
{
    report[firstByte + (size_t)( bit / 8 )] |= (uint8_t)( 1 << ( bit % 8 ) );
}

static void testOpen()
{
    CHECK( dbm_nihost_api_version() == DBM_NIHOST_API_VERSION );
    CHECK( dbm_nihost_open_test( 0 ) == nullptr );
    CHECK( dbm_nihost_open( 3 ) == nullptr );
    dbm_nihost_close( nullptr );
    CHECK( dbm_nihost_is_attached( nullptr ) == 0 );
}

static void testMk2()
{
    dbm_nihost* device = dbm_nihost_open_test( DBM_NIHOST_MK2 );
    CHECK( device != nullptr );
    CHECK( dbm_nihost_is_attached( device ) == 0 );
    // No device: no answer to the query
    send( device, query );
    CHECK( readAll( device ).empty() );

    // Attached: the hello, also the answer to the query
    dbm_nihost_test_attach( device, 1 );
    CHECK( dbm_nihost_is_attached( device ) == 1 );
    std::vector<Bytes> midi = readAll( device );
    CHECK( has( midi, mk2Hello ) );
    send( device, query );
    midi = readAll( device );
    CHECK( midi.size() == 1 && has( midi, mk2Hello ) );

    // Buttons: Shift as the DrivenByMoss sysex, PLAY as its CC, encoder 1
    Bytes report( 25, 0 );
    report[0] = 0x01;
    dbm_nihost_test_report( device, report.data(), report.size() );
    readAll( device );
    setBit( report, 1, MaschineMk2::Shift );
    setBit( report, 1, MaschineMk2::Play );
    report[9] = 11;
    dbm_nihost_test_report( device, report.data(), report.size() );
    midi = readAll( device );
    CHECK( has( midi, { 0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x12, 0x01, 0xF7 } ) );
    CHECK( has( midi, { 0xB0, 57, 127 } ) );
    CHECK( has( midi, { 0xB0, 70, 1 } ) );
    // Several messages in one read, each with its length
    {
        uint8_t   buffer[256];
        report.assign( report.size(), 0 );
        report[0] = 0x01;
        report[9] = 11;
        dbm_nihost_test_report( device, report.data(), report.size() );
        const int length = dbm_nihost_read( device, buffer, sizeof buffer, 0 );
        CHECK( length == 2 + 8 + 2 + 3 ); // Shift up, PLAY up
        // in the order of the buttons
        const int first = buffer[0] | ( buffer[1] << 8 );
        CHECK( ( first == 3 && buffer[2] == 0xB0 && buffer[5] == 8 && buffer[7] == 0xF0 )
               || ( first == 8 && buffer[2] == 0xF0 && buffer[10] == 3 && buffer[12] == 0xB0 ) );
    }
    // A buffer too small for the next message
    {
        uint8_t small[4];
        dbm_nihost_test_attach( device, 1 ); // the hello again
        CHECK( dbm_nihost_read( device, small, sizeof small, 0 ) == -1 );
        CHECK( readAll( device ).size() == 1 );
    }

    // Pads: report 0x20, the pad in the top 4 bits; pad 0 (top left) is note 48
    Bytes pads( 65, 0 );
    pads[0] = 0x20;
    for( int i = 0; i < 4; i++ )
    {
        const int word = ( 0 << 12 ) | 3000;
        pads[1]        = (uint8_t)( word & 0xFF );
        pads[2]        = (uint8_t)( word >> 8 );
        dbm_nihost_test_report( device, pads.data(), pads.size() );
    }
    bool noteOn = false;
    for( const Bytes& m : readAll( device ) )
        noteOn |= m.size() == 3 && m[0] == 0x90 && m[1] == 48 && m[2] > 0;
    CHECK( noteOn );

    // LEDs and displays from the DAW
    CHECK( dbm_nihost_flush( device ) >= 0 );
    takeReports( device );
    send( device, { 0x90, 48, 6 } );   // pad top left red
    send( device, { 0xB0, 57, 127 } ); // PLAY
    Bytes text = { 0xF0, 0x00, 0x00, 0x66, 0x14, 0x12, 0x00, 'H', 'e', 'l', 'l', 'o', 0xF7 };
    send( device, text );
    std::this_thread::sleep_for( std::chrono::milliseconds( 60 ) );
    const int written = dbm_nihost_flush( device );
    std::vector<Bytes> reports = takeReports( device );
    CHECK( written > 0 && (size_t)written == reports.size() );
    const Bytes* padReport = findReport( reports, 0x80 );
    CHECK( padReport != nullptr && ( *padReport )[1] == 255 / 2 && ( *padReport )[2] == 0 );
    CHECK( findReport( reports, 0xE0 ) != nullptr ); // a display chunk
    // Nothing changed: nothing written
    CHECK( dbm_nihost_flush( device ) == 0 );

    // Unplugged: gone, no answer to the query
    dbm_nihost_test_attach( device, 0 );
    CHECK( dbm_nihost_is_attached( device ) == 0 );
    midi = readAll( device );
    CHECK( midi.size() == 1 && has( midi, gone ) );
    send( device, query );
    CHECK( readAll( device ).empty() );
    dbm_nihost_close( device );
}

static void testJam()
{
    dbm_nihost* device = dbm_nihost_open_test( DBM_NIHOST_JAM );
    CHECK( device != nullptr );
    dbm_nihost_test_attach( device, 1 );
    std::vector<Bytes> midi = readAll( device );
    CHECK( midi.size() == 1 && has( midi, jamHello ) );
    send( device, query );
    CHECK( has( readAll( device ), jamHello ) );

    Bytes report( 2 + MaschineJam::numButtonBytes, 0 );
    report[0] = 0x01;
    report[1] = 3;
    dbm_nihost_test_report( device, report.data(), report.size() );
    readAll( device );
    setBit( report, 2, MaschineJam::Play );
    setBit( report, 2, MaschineJam::Pad1 );
    dbm_nihost_test_report( device, report.data(), report.size() );
    midi = readAll( device );
    CHECK( has( midi, { 0xB0, 90, 127 } ) );
    CHECK( has( midi, { 0x90, 92, 127 } ) );

    takeReports( device );
    dbm_nihost_flush( device );
    takeReports( device );
    send( device, { 0x90, 92, 6 } );
    send( device, { 0xB0, 90, 127 } );
    CHECK( dbm_nihost_flush( device ) > 0 );
    std::vector<Bytes> reports = takeReports( device );
    const Bytes*       rgb     = findReport( reports, 0x81 );
    CHECK( rgb != nullptr && ( *rgb )[1 + 8] == 6 );
    const Bytes* mono = findReport( reports, 0x80 );
    CHECK( mono != nullptr && ( *mono )[1 + MaschineJam::LedPlay] == 0x7F );

    dbm_nihost_test_attach( device, 0 );
    CHECK( has( readAll( device ), gone ) );
    dbm_nihost_close( device );
}

// A reader blocked in dbm_nihost_read wakes up when a message comes, and times out without one
static void testBlockingRead()
{
    dbm_nihost* device = dbm_nihost_open_test( DBM_NIHOST_MK2 );
    uint8_t     buffer[64];
    auto        start  = std::chrono::steady_clock::now();
    CHECK( dbm_nihost_read( device, buffer, sizeof buffer, 50 ) == 0 );
    CHECK( std::chrono::steady_clock::now() - start >= std::chrono::milliseconds( 45 ) );

    std::thread attach( [device] {
        std::this_thread::sleep_for( std::chrono::milliseconds( 30 ) );
        dbm_nihost_test_attach( device, 1 );
    } );
    start            = std::chrono::steady_clock::now();
    const int length = dbm_nihost_read( device, buffer, sizeof buffer, 2000 );
    CHECK( length > 0 );
    CHECK( std::chrono::steady_clock::now() - start < std::chrono::milliseconds( 1000 ) );
    attach.join();
    dbm_nihost_close( device );
}

// True if a JAM is connected (HID_ID ...:000017CC:00001500 of a hidraw device)
static bool jamConnected()
{
    for( int i = 0; i < 64; i++ )
    {
        char path[64];
        std::snprintf( path, sizeof path, "/sys/class/hidraw/hidraw%d/device/uevent", i );
        std::FILE* file = std::fopen( path, "r" );
        if( file == nullptr )
            continue;
        char line[256];
        bool found = false;
        while( std::fgets( line, sizeof line, file ) != nullptr )
            found |= std::strstr( line, "HID_ID=0003:000017CC:00001500" ) != nullptr;
        std::fclose( file );
        if( found )
            return true;
    }
    return false;
}

// A real device (hidraw) without the hardware: it looks for it, is not attached, closes at once.
// Skipped while a JAM is connected (it would open it, beside another process driving it).
static void testRealWithoutHardware()
{
    if( jamConnected() )
    {
        std::printf( "A JAM is connected: the test without hardware is skipped.\n" );
        return;
    }
    const auto  start  = std::chrono::steady_clock::now();
    dbm_nihost* device = dbm_nihost_open( DBM_NIHOST_JAM );
    CHECK( device != nullptr );
    uint8_t buffer[64];
    dbm_nihost_read( device, buffer, sizeof buffer, 20 );
    dbm_nihost_close( device );
    CHECK( std::chrono::steady_clock::now() - start < std::chrono::milliseconds( 1000 ) );
}

int main()
{
    testOpen();
    testMk2();
    testJam();
    testBlockingRead();
    testRealWithoutHardware();
    if( failures == 0 )
        std::printf( "All libdbm_nihost tests passed.\n" );
    return failures == 0 ? 0 : 1;
}
