// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
//
// Ported from KICK's kick_controllers module (same author, relicensed from AGPL-3.0).

#include "hidraw_device.h"

#include "log.h"

#include <cerrno>
#include <chrono>
#include <cstdio>
#include <cstring>
#include <dirent.h>
#include <fcntl.h>
#include <fstream>
#include <poll.h>
#include <unistd.h>

namespace dbm::nihost
{
    HidrawDevice::HidrawDevice( int VendorId, int ProductId, std::string Name, Listener& L )
        : vendorId( VendorId ), productId( ProductId ), name( std::move( Name ) ), listener( L )
    {
    }

    HidrawDevice::~HidrawDevice()
    {
        stop();
    }

    void HidrawDevice::start()
    {
        if( this->running.exchange( true ) )
            return;
        this->thread = std::thread( [this] { this->run(); } );
    }

    void HidrawDevice::stop()
    {
        this->running = false;
        if( this->thread.joinable() )
            this->thread.join();
    }

    bool HidrawDevice::write( const uint8_t* report, size_t length )
    {
        std::lock_guard<std::mutex> lock( this->fdLock );
        return this->fd >= 0 && ::write( this->fd, report, length ) == (ssize_t)length;
    }

    std::string HidrawDevice::findDevice( int vendorId, int productId, const std::string& sysfsRoot )
    {
        DIR* dir = ::opendir( sysfsRoot.c_str() );
        if( dir == nullptr )
            return {};
        std::string result;
        while( const dirent* entry = ::readdir( dir ) )
        {
            const std::string device = entry->d_name;
            if( device.rfind( "hidraw", 0 ) != 0 )
                continue;
            // HID_ID=0003:000017CC:00001500 (bus, vendor, product)
            std::ifstream uevent( sysfsRoot + "/" + device + "/device/uevent" );
            std::string   line;
            while( std::getline( uevent, line ) )
            {
                unsigned bus, vendor, product;
                if( std::sscanf( line.c_str(), "HID_ID=%x:%x:%x", &bus, &vendor, &product ) == 3
                    && (int)vendor == vendorId && (int)product == productId )
                {
                    result = "/dev/" + device;
                    break;
                }
            }
            if( !result.empty() )
                break;
        }
        ::closedir( dir );
        return result;
    }

    bool HidrawDevice::sleepFor( int milliseconds )
    {
        for( int waited = 0; waited < milliseconds && this->running; waited += 50 )
            std::this_thread::sleep_for( std::chrono::milliseconds( 50 ) );
        return this->running;
    }

    void HidrawDevice::run()
    {
        bool warned = false;

        while( this->running )
        {
            const std::string path   = findDevice( this->vendorId, this->productId );
            const int         handle = path.empty() ? -1 : ::open( path.c_str(), O_RDWR | O_CLOEXEC );
            if( handle < 0 )
            {
                if( !path.empty() && !warned )
                {
                    char vendor[8];
                    std::snprintf( vendor, sizeof vendor, "%04x", this->vendorId );
                    log( "Cannot open the " + this->name + " at " + path + " (" + std::strerror( errno )
                         + "); a udev rule gives access: KERNEL==\"hidraw*\", ATTRS{idVendor}==\"" + vendor
                         + "\", MODE=\"0660\", TAG+=\"uaccess\"" );
                    warned = true;
                }
                if( !this->sleepFor( 1000 ) )
                    break;
                continue;
            }

            {
                std::lock_guard<std::mutex> lock( this->fdLock );
                this->fd = handle;
            }
            this->opened = true;
            log( this->name + " at " + path );
            this->listener.hidAttached();

            uint8_t buffer[512];
            while( this->running )
            {
                pollfd    p{ handle, POLLIN, 0 };
                const int ready = ::poll( &p, 1, 100 );
                if( ready < 0 && errno == EINTR )
                    continue;
                if( ready < 0 || ( p.revents & ( POLLERR | POLLHUP | POLLNVAL ) ) )
                    break; // unplugged
                if( ready == 0 )
                    continue;

                const auto length = ::read( handle, buffer, sizeof buffer );
                if( length <= 0 )
                    break;
                this->listener.hidReport( buffer, (size_t)length );
            }

            {
                std::lock_guard<std::mutex> lock( this->fdLock );
                this->fd = -1;
            }
            this->opened = false;
            ::close( handle );
            this->listener.hidDetached();
            if( this->running )
                log( this->name + " at " + path + " is gone" );
            warned = false;
        }
    }
}
