// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#pragma once

#include <cstddef>
#include <cstdint>
#include <functional>
#include <mutex>
#include <string>
#include <vector>

struct _snd_seq;
struct snd_midi_event;

namespace dbm::nihost
{
    // An ALSA sequencer client with an input and an output port, like the MIDI ports NI's
    // driver creates for a device: the DAW reads from inputName (the device's MIDI) and writes
    // to outputName (LEDs, displays).
    class AlsaPorts
    {
    public:
        // Called with each complete MIDI message (sysex collected) the DAW sent.
        using Receiver = std::function<void( const uint8_t* data, size_t length )>;

        AlsaPorts( const std::string& clientName, const std::string& inputName, const std::string& outputName );
        ~AlsaPorts();

        AlsaPorts( const AlsaPorts& )            = delete;
        AlsaPorts& operator=( const AlsaPorts& ) = delete;

        bool isOpen() const { return this->seq != nullptr; }

        // Send a MIDI message to the subscribers of the input port; from any thread.
        void send( const uint8_t* data, size_t length );

        // Wait up to the timeout for messages from the DAW and hand them to the receiver.
        void receive( int timeoutMilliseconds, const Receiver& receiver );

    private:
        _snd_seq*       seq{ nullptr };
        snd_midi_event* encoder{ nullptr };
        snd_midi_event* decoder{ nullptr };
        int             inputPort{ -1 }, outputPort{ -1 };
        std::mutex      sendLock;
        std::vector<uint8_t> sysex;
    };
}
