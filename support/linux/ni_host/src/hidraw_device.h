// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
//
// Ported from KICK's kick_controllers module (same author, relicensed from AGPL-3.0); uses
// std::thread instead of JUCE.

#pragma once

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <mutex>
#include <string>
#include <thread>

namespace dbm::nihost
{
    // A USB HID device through Linux hidraw (/dev/hidrawN), found by its vendor and product id.
    // A thread of its own looks for it once a second, opens it and reads its input reports, so
    // it can be plugged in and out at any time. Opening needs read and write access, e.g. the
    // udev rule
    //   KERNEL=="hidraw*", ATTRS{idVendor}=="17cc", MODE="0660", TAG+="uaccess"
    class HidrawDevice
    {
    public:
        // All three are called on the reader thread.
        struct Listener
        {
            virtual ~Listener() = default;
            virtual void hidAttached()                                     = 0;
            virtual void hidDetached()                                     = 0;
            virtual void hidReport( const uint8_t* report, size_t length ) = 0;
        };

        HidrawDevice( int vendorId, int productId, std::string name, Listener& listener );
        ~HidrawDevice();

        HidrawDevice( const HidrawDevice& )            = delete;
        HidrawDevice& operator=( const HidrawDevice& ) = delete;

        void start();
        // stops the reader thread (it calls the listener no more)
        void stop();

        // An output report (report[0] = report ID); from any thread. False if no device is
        // open or the write failed.
        bool write( const uint8_t* report, size_t length );

        bool isOpen() const { return this->opened.load(); }

        // /dev/hidrawN of the first device with the vendor and product id, "" if there is none.
        // sysfsRoot is "/sys/class/hidraw" except in tests.
        static std::string findDevice( int vendorId, int productId, const std::string& sysfsRoot = "/sys/class/hidraw" );

    private:
        void run();
        // Waits up to the given time; false if the thread should stop.
        bool sleepFor( int milliseconds );

        const int         vendorId, productId;
        const std::string name;
        Listener&         listener;

        std::thread       thread;
        std::atomic<bool> running{ false };
        std::atomic<bool> opened{ false };

        std::mutex fdLock;
        int        fd{ -1 };
    };
}
