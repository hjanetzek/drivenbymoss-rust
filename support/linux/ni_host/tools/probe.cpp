// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
//
// nihost-probe: finds the Maschine Mk2 and JAM over hidraw, prints their decoded input events
// and lights a test pattern (pads in rainbow colours, all button LEDs dim, groups lit).
// Stop with Ctrl+C.

#include "hidraw_device.h"
#include "log.h"
#include "maschine_jam.h"
#include "maschine_mk2.h"

#include <atomic>
#include <chrono>
#include <csignal>
#include <cstdio>
#include <mutex>
#include <thread>

using namespace dbm::nihost;

static std::atomic<bool> running{ true };

// Owns a protocol class and its hidraw device. The protocol class is not thread safe, so the
// reader thread (decode) and the main thread (LEDs, flush) share it under a lock.
template <typename Protocol>
class Probe : public HidrawDevice::Listener
{
public:
    explicit Probe( const char* name ) : device( Protocol::vendorId, Protocol::productId, name, *this ) {}

    void start() { this->device.start(); }
    void stop() { this->device.stop(); }

    template <typename F>
    void withProtocol( F f )
    {
        std::lock_guard<std::mutex> lock( this->protocolLock );
        f( this->protocol );
    }

    void flush()
    {
        std::lock_guard<std::mutex> lock( this->protocolLock );
        this->protocol.flush( [this]( const uint8_t* data, size_t length ) { return this->device.write( data, length ); } );
    }

    std::atomic<bool> attached{ false };

protected:
    void hidAttached() override
    {
        withProtocol( []( Protocol& p ) { p.reset(); } );
        this->attached = true;
    }

    void hidDetached() override { this->attached = false; }

    std::mutex   protocolLock;
    Protocol     protocol;
    HidrawDevice device;
};


class JamProbe : public Probe<MaschineJam>, public MaschineJam::Listener
{
public:
    JamProbe() : Probe( "Maschine JAM" ) {}

    void hidReport( const uint8_t* report, size_t length ) override
    {
        withProtocol( [&]( MaschineJam& p ) { p.decode( report, length, *this ); } );
    }

    void jamButton( int button, bool down ) override { std::printf( "JAM button %d %s\n", button, down ? "down" : "up" ); }
    void jamEncoder( int steps ) override { std::printf( "JAM encoder %+d\n", steps ); }
    void jamStrip( int strip, int position ) override { std::printf( "JAM strip %d %d\n", strip + 1, position ); }

    void pattern()
    {
        withProtocol( []( MaschineJam& p ) {
            static const int hues[] = { MaschineJam::HueRed, MaschineJam::HueOrange, MaschineJam::HueYellow, MaschineJam::HueGreen,
                                        MaschineJam::HueCyan, MaschineJam::HueBlue, MaschineJam::HuePurple, MaschineJam::HuePink };
            for( int y = 0; y < 8; y++ )
                for( int x = 0; x < 8; x++ )
                    p.setPad( x, y, MaschineJam::colour( hues[x], y < 4 ? MaschineJam::LevelFull : MaschineJam::LevelDim ) );
            for( int i = 0; i < 8; i++ )
            {
                p.setGroup( i, MaschineJam::colour( hues[i], MaschineJam::LevelFull ) );
                p.setScene( i, MaschineJam::colour( MaschineJam::HueWhite, MaschineJam::LevelDim ) );
                p.setStripBar( i, i + 1, MaschineJam::colour( hues[i], MaschineJam::LevelFull ) );
            }
            for( int led = 0; led < MaschineJam::NumMonoLeds; led++ )
                p.setMono( led, MaschineJam::lightDim );
        } );
    }
};


class Mk2Probe : public Probe<MaschineMk2>, public MaschineMk2::Listener
{
public:
    Mk2Probe() : Probe( "Maschine Mk2" ) {}

    void hidReport( const uint8_t* report, size_t length ) override
    {
        withProtocol( [&]( MaschineMk2& p ) { p.decode( report, length, *this ); } );
    }

    void mk2Button( int button, bool down ) override { std::printf( "Mk2 button %d %s\n", button, down ? "down" : "up" ); }
    void mk2MainEncoder( int steps ) override { std::printf( "Mk2 main encoder %+d\n", steps ); }
    void mk2Encoder( int encoder, int delta ) override { std::printf( "Mk2 encoder %d %+d\n", encoder + 1, delta ); }
    void mk2Pad( int pad, bool down, int velocity ) override { std::printf( "Mk2 pad %d %s %d\n", pad + 1, down ? "down" : "up", velocity ); }

    void pattern()
    {
        withProtocol( []( MaschineMk2& p ) {
            for( int pad = 0; pad < MaschineMk2::numPads; pad++ )
            {
                // a simple hue wheel
                const int     h = pad * 6 / MaschineMk2::numPads, f = ( pad * 6 * 0x7F / MaschineMk2::numPads ) % 0x80;
                const uint8_t q = (uint8_t)( 0x7F - f ), t = (uint8_t)f, v = 0x7F;
                const uint8_t rgb[6][3] = { { v, t, 0 }, { q, v, 0 }, { 0, v, t }, { 0, q, v }, { t, 0, v }, { v, 0, q } };
                p.setPad( pad, rgb[h][0], rgb[h][1], rgb[h][2] );
            }
            for( int group = 0; group < 8; group++ )
                p.setGroup( group, 0x7F, 0x40, 0 );
            for( int led = 0; led < MaschineMk2::NumLeds; led++ )
                p.setLed( led, MaschineMk2::lightDim );
            // a frame on both displays
            MaschineMk2::Display display{};
            for( int x = 0; x < MaschineMk2::displayWidth / 8; x++ )
            {
                display[x]                                                             = 0xFF;
                display[( MaschineMk2::displayHeight - 1 ) * MaschineMk2::displayWidth / 8 + x] = 0xFF;
            }
            for( int y = 0; y < MaschineMk2::displayHeight; y++ )
            {
                display[y * MaschineMk2::displayWidth / 8] |= 0x80;
                display[y * MaschineMk2::displayWidth / 8 + MaschineMk2::displayWidth / 8 - 1] |= 0x01;
            }
            p.setDisplay( 0, display );
            p.setDisplay( 1, display );
        } );
    }
};


int main()
{
    std::signal( SIGINT, []( int ) { running = false; } );
    std::setvbuf( stdout, nullptr, _IOLBF, 0 );

    JamProbe jam;
    Mk2Probe mk2;
    jam.start();
    mk2.start();

    bool jamLit = false, mk2Lit = false;
    while( running )
    {
        if( jam.attached && !jamLit )
            jam.pattern();
        if( mk2.attached && !mk2Lit )
            mk2.pattern();
        jamLit = jam.attached;
        mk2Lit = mk2.attached;
        jam.flush();
        mk2.flush();
        std::this_thread::sleep_for( std::chrono::milliseconds( 10 ) );
    }

    // turn everything off
    jam.withProtocol( []( MaschineJam& p ) { p.reset(); } );
    mk2.withProtocol( []( MaschineMk2& p ) { p.reset(); } );
    jam.flush();
    mk2.flush();
    jam.stop();
    mk2.stop();
    return 0;
}
