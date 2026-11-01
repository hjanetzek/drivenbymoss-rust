// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
//
// dbm-nihost: emulates NI's driver on Linux. For each supported Maschine it creates the MIDI
// ports NI's driver creates on Windows and macOS and translates between them and the device's
// USB HID reports, with the numbering of the DrivenByMoss Controller Editor templates.
// With --virmidi the ports are also connected to the virtual raw MIDI devices of snd-virmidi, for
// DAWs that only open raw MIDI devices (Bitwig): in the order Mk2, JAM, as far as they are enabled.
// Stop with Ctrl+C or SIGTERM.

#include "alsa_ports.h"
#include "hidraw_device.h"
#include "jam_host.h"
#include "log.h"
#include "maschine_jam.h"
#include "maschine_mk2.h"
#include "mk2_host.h"

#include <atomic>
#include <chrono>
#include <csignal>
#include <cstring>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

using namespace dbm::nihost;

namespace
{
    std::atomic<bool> running{ true };

    bool logMidi = false;

    // The name prefix of the sequencer clients of snd-virmidi
    const std::string virmidiPrefix = "Virtual Raw MIDI";

    std::string hex( const uint8_t* data, size_t length )
    {
        std::string text;
        char        byte[4];
        for( size_t i = 0; i < length && i < 32; i++ )
        {
            std::snprintf( byte, sizeof byte, "%02X ", data[i] );
            text += byte;
        }
        if( length > 32 )
            text += "...";
        return text;
    }

    // A Maschine: its hidraw device, the translation and the ALSA ports.
    class Device
    {
    public:
        virtual ~Device() = default;
        virtual void start() = 0;
        virtual void stop() = 0;
        // Receive MIDI from the DAW, then send the changed LED reports; called every few ms.
        virtual void poll() = 0;
        virtual const std::string& getName() const = 0;
        virtual AlsaPorts& getPorts() = 0;
        // The ports were connected to the DAW (snd-virmidi).
        virtual void dawConnected() = 0;
    };

    template <typename Host, typename Protocol>
    class DeviceImpl : public Device, private HidrawDevice::Listener
    {
    public:
        DeviceImpl( const std::string& name, const std::string& inputName, const std::string& outputName )
            : name( name ), ports( name, inputName, outputName ), host( [this]( const uint8_t* data, size_t length ) { this->toDaw( data, length ); } ),
              hidraw( Protocol::vendorId, Protocol::productId, name, *this )
        {
        }

        const std::string& getName() const override { return this->name; }
        AlsaPorts& getPorts() override { return this->ports; }

        void start() override { this->hidraw.start(); }
        void dawConnected() override
        {
            std::lock_guard<std::mutex> lock( this->hostLock );
            this->host.dawConnected();
        }
        void stop() override { this->hidraw.stop(); }

        void poll() override
        {
            this->ports.receive( 5, [this]( const uint8_t* data, size_t length ) {
                if( logMidi )
                    log( this->name + " <- DAW " + hex( data, length ) );
                std::lock_guard<std::mutex> lock( this->hostLock );
                this->host.midiIn( data, length );
            } );
            std::lock_guard<std::mutex> lock( this->hostLock );
            this->host.flush( [this]( const uint8_t* report, size_t length ) { return this->hidraw.write( report, length ); } );
        }

    private:
        void hidAttached() override
        {
            std::lock_guard<std::mutex> lock( this->hostLock );
            this->host.attached();
        }

        void hidDetached() override {}

        void hidReport( const uint8_t* report, size_t length ) override
        {
            std::lock_guard<std::mutex> lock( this->hostLock );
            this->host.hidReport( report, length );
        }

        void toDaw( const uint8_t* data, size_t length )
        {
            if( logMidi )
                log( this->name + " -> DAW " + hex( data, length ) );
            this->ports.send( data, length );
        }

        const std::string name;
        AlsaPorts         ports;
        std::mutex        hostLock;
        Host              host;
        HidrawDevice      hidraw;
    };

    // Connect the devices in order to the snd-virmidi clients they are not yet connected to.
    // connected holds the client of each device, -1 if none. A removed client drops its
    // connections, so checking again later also handles loading the module after the start.
    void connectVirmidi( std::vector<std::unique_ptr<Device>>& devices, std::vector<int>& connected )
    {
        const std::vector<int> clients = devices.front()->getPorts().findClients( virmidiPrefix );
        for( size_t i = 0; i < devices.size(); i++ )
        {
            const int client = i < clients.size() ? clients[i] : -1;
            if( client == connected[i] )
                continue;
            connected[i] = -1;
            if( client < 0 )
                continue;
            if( devices[i]->getPorts().connect( client, 0 ) )
            {
                connected[i] = client;
                log( devices[i]->getName() + " connected to " + virmidiPrefix + " client " + std::to_string( client ) );
                devices[i]->dawConnected();
            }
            else
                log( "Cannot connect " + devices[i]->getName() + " to " + virmidiPrefix + " client " + std::to_string( client ) );
        }
    }

    void usage()
    {
        std::fprintf( stderr, "Usage: dbm-nihost [--mk2] [--jam] [--virmidi] [--log-midi]\n"
                              "Without --mk2 / --jam both devices are supported.\n"
                              "--virmidi connects them to the devices of snd-virmidi (for Bitwig).\n" );
    }
}


int main( int argc, char** argv )
{
    bool mk2 = false, jam = false, virmidi = false;
    for( int i = 1; i < argc; i++ )
    {
        if( std::strcmp( argv[i], "--mk2" ) == 0 )
            mk2 = true;
        else if( std::strcmp( argv[i], "--jam" ) == 0 )
            jam = true;
        else if( std::strcmp( argv[i], "--virmidi" ) == 0 )
            virmidi = true;
        else if( std::strcmp( argv[i], "--log-midi" ) == 0 )
            logMidi = true;
        else
        {
            usage();
            return 1;
        }
    }
    if( !mk2 && !jam )
        mk2 = jam = true;

    std::signal( SIGINT, []( int ) { running = false; } );
    std::signal( SIGTERM, []( int ) { running = false; } );

    std::vector<std::unique_ptr<Device>> devices;
    if( mk2 )
        devices.push_back( std::make_unique<DeviceImpl<Mk2Host, MaschineMk2>>( "Maschine MK2", "Maschine MK2 In", "Maschine MK2 Out" ) );
    if( jam )
        devices.push_back( std::make_unique<DeviceImpl<JamHost, MaschineJam>>( "Maschine Jam - 1", "Maschine Jam - 1 Input", "Maschine Jam - 1 Output" ) );

    for( auto& device : devices )
        device->start();

    std::vector<int> virmidiClients( devices.size(), -1 );
    auto             lastVirmidiCheck = std::chrono::steady_clock::time_point();
    while( running )
    {
        for( auto& device : devices )
            device->poll();

        const auto now = std::chrono::steady_clock::now();
        if( virmidi && now - lastVirmidiCheck >= std::chrono::seconds( 2 ) )
        {
            lastVirmidiCheck = now;
            connectVirmidi( devices, virmidiClients );
        }
    }

    for( auto& device : devices )
        device->stop();
    return 0;
}
