// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
//
// libdbm_nihost: the C interface of include/dbm_nihost.h. A device is the host of daemon.cpp
// without the ALSA ports: its MIDI to the DAW goes into a queue which the caller reads.

#include "dbm_nihost.h"

#include "hidraw_device.h"
#include "jam_host.h"
#include "maschine_jam.h"
#include "maschine_mk2.h"
#include "mk2_host.h"

#include <chrono>
#include <condition_variable>
#include <cstring>
#include <deque>
#include <memory>
#include <mutex>
#include <string>
#include <vector>

#define DBM_NIHOST_EXPORT extern "C" __attribute__( ( visibility( "default" ) ) )

using namespace dbm::nihost;

struct dbm_nihost
{
    virtual ~dbm_nihost() = default;
    virtual void start() = 0;
    virtual void stop() = 0;
    virtual bool isAttached() = 0;
    virtual void midiIn( const uint8_t* data, size_t length ) = 0;
    virtual int flush() = 0;
    virtual void testAttach( bool attached ) = 0;
    virtual void testReport( const uint8_t* report, size_t length ) = 0;
    virtual int takeTestReport( uint8_t* buffer, size_t size ) = 0;

    // The MIDI to the DAW
    int read( uint8_t* buffer, size_t size, int timeoutMs )
    {
        std::unique_lock<std::mutex> lock( this->queueLock );
        if( this->queue.empty() && timeoutMs > 0 )
            this->queueChanged.wait_for( lock, std::chrono::milliseconds( timeoutMs ), [this] { return !this->queue.empty(); } );
        size_t used = 0;
        while( !this->queue.empty() )
        {
            const std::vector<uint8_t>& message = this->queue.front();
            if( used + 2 + message.size() > size )
            {
                if( used == 0 )
                    return -1;
                break;
            }
            buffer[used++] = (uint8_t)( message.size() & 0xFF );
            buffer[used++] = (uint8_t)( message.size() >> 8 );
            std::memcpy( buffer + used, message.data(), message.size() );
            used += message.size();
            this->queue.pop_front();
        }
        return (int)used;
    }

protected:
    void toDaw( const uint8_t* data, size_t length )
    {
        {
            std::lock_guard<std::mutex> lock( this->queueLock );
            if( this->queue.size() >= maxQueued )
                this->queue.pop_front();
            this->queue.emplace_back( data, data + length );
        }
        this->queueChanged.notify_all();
    }

private:
    static constexpr size_t             maxQueued = 4096;
    std::mutex                          queueLock;
    std::condition_variable             queueChanged;
    std::deque<std::vector<uint8_t>>    queue;
};

namespace
{
    template <typename Host, typename Protocol>
    class Device : public dbm_nihost, private HidrawDevice::Listener
    {
    public:
        Device( const char* name, bool test )
            : host( [this]( const uint8_t* data, size_t length ) { this->toDaw( data, length ); } )
        {
            if( !test )
                this->hidraw = std::make_unique<HidrawDevice>( Protocol::vendorId, Protocol::productId, name, static_cast<HidrawDevice::Listener&>( *this ) );
        }

        ~Device() override { this->stop(); }

        void start() override
        {
            if( this->hidraw )
                this->hidraw->start();
        }

        void stop() override
        {
            if( this->hidraw )
                this->hidraw->stop();
        }

        bool isAttached() override
        {
            if( this->hidraw )
                return this->hidraw->isOpen();
            std::lock_guard<std::mutex> lock( this->hostLock );
            return this->testAttached;
        }

        void midiIn( const uint8_t* data, size_t length ) override
        {
            std::lock_guard<std::mutex> lock( this->hostLock );
            this->host.midiIn( data, length );
        }

        int flush() override
        {
            int  written = 0;
            bool failed  = false;
            // Not attached: nothing to write (attaching sends everything anyway)
            if( this->hidraw && !this->hidraw->isOpen() )
                return 0;
            std::lock_guard<std::mutex> lock( this->hostLock );
            this->host.flush( [&]( const uint8_t* report, size_t length ) {
                bool ok;
                if( this->hidraw )
                    ok = this->hidraw->write( report, length );
                else
                {
                    this->testReports.emplace_back( report, report + length );
                    ok = true;
                }
                if( ok )
                    written++;
                else
                    failed = true;
                return ok;
            } );
            return failed ? -1 : written;
        }

        void testAttach( bool attached ) override
        {
            if( this->hidraw )
                return;
            {
                std::lock_guard<std::mutex> lock( this->hostLock );
                this->testAttached = attached;
            }
            if( attached )
                this->hidAttached();
            else
                this->hidDetached();
        }

        void testReport( const uint8_t* report, size_t length ) override
        {
            if( !this->hidraw )
                this->hidReport( report, length );
        }

        int takeTestReport( uint8_t* buffer, size_t size ) override
        {
            std::lock_guard<std::mutex> lock( this->hostLock );
            if( this->testReports.empty() )
                return 0;
            const std::vector<uint8_t>& report = this->testReports.front();
            if( report.size() > size )
                return -1;
            std::memcpy( buffer, report.data(), report.size() );
            const int length = (int)report.size();
            this->testReports.pop_front();
            return length;
        }

    private:
        // On the reader thread of HidrawDevice
        void hidAttached() override
        {
            std::lock_guard<std::mutex> lock( this->hostLock );
            this->host.attached();
        }

        void hidDetached() override
        {
            std::lock_guard<std::mutex> lock( this->hostLock );
            this->host.detached();
        }

        void hidReport( const uint8_t* report, size_t length ) override
        {
            std::lock_guard<std::mutex> lock( this->hostLock );
            this->host.hidReport( report, length );
        }

        std::mutex                          hostLock;
        Host                                host;
        bool                                testAttached{ false };
        std::deque<std::vector<uint8_t>>    testReports;
        // Last: its thread stops (in stop()) before the host goes
        std::unique_ptr<HidrawDevice>       hidraw;
    };

    dbm_nihost* create( int kind, bool test )
    {
        try
        {
            std::unique_ptr<dbm_nihost> device;
            if( kind == DBM_NIHOST_MK2 )
                device = std::make_unique<Device<Mk2Host, MaschineMk2>>( "Maschine MK2", test );
            else if( kind == DBM_NIHOST_JAM )
                device = std::make_unique<Device<JamHost, MaschineJam>>( "Maschine Jam - 1", test );
            else
                return nullptr;
            device->start();
            return device.release();
        }
        catch( ... )
        {
            return nullptr;
        }
    }
}

DBM_NIHOST_EXPORT int dbm_nihost_api_version( void )
{
    return DBM_NIHOST_API_VERSION;
}

DBM_NIHOST_EXPORT dbm_nihost* dbm_nihost_open( int kind )
{
    return create( kind, false );
}

DBM_NIHOST_EXPORT dbm_nihost* dbm_nihost_open_test( int kind )
{
    return create( kind, true );
}

DBM_NIHOST_EXPORT void dbm_nihost_close( dbm_nihost* device )
{
    if( device == nullptr )
        return;
    device->stop();
    delete device;
}

DBM_NIHOST_EXPORT int dbm_nihost_is_attached( dbm_nihost* device )
{
    return device != nullptr && device->isAttached() ? 1 : 0;
}

DBM_NIHOST_EXPORT int dbm_nihost_read( dbm_nihost* device, uint8_t* buffer, size_t size, int timeout_ms )
{
    if( device == nullptr || buffer == nullptr )
        return -1;
    return device->read( buffer, size, timeout_ms );
}

DBM_NIHOST_EXPORT int dbm_nihost_send( dbm_nihost* device, const uint8_t* data, size_t length )
{
    if( device == nullptr || data == nullptr || length == 0 )
        return -1;
    try
    {
        device->midiIn( data, length );
    }
    catch( ... )
    {
        return -1;
    }
    return 0;
}

DBM_NIHOST_EXPORT int dbm_nihost_flush( dbm_nihost* device )
{
    if( device == nullptr )
        return -1;
    try
    {
        return device->flush();
    }
    catch( ... )
    {
        return -1;
    }
}

DBM_NIHOST_EXPORT void dbm_nihost_test_attach( dbm_nihost* device, int attached )
{
    if( device != nullptr )
        device->testAttach( attached != 0 );
}

DBM_NIHOST_EXPORT void dbm_nihost_test_report( dbm_nihost* device, const uint8_t* report, size_t length )
{
    if( device != nullptr && report != nullptr )
        device->testReport( report, length );
}

DBM_NIHOST_EXPORT int dbm_nihost_test_take_report( dbm_nihost* device, uint8_t* buffer, size_t size )
{
    if( device == nullptr || buffer == nullptr )
        return -1;
    return device->takeTestReport( buffer, size );
}
