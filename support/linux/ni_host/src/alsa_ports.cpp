// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#include "alsa_ports.h"

#include "log.h"

#include <alsa/asoundlib.h>
#include <poll.h>

namespace dbm::nihost
{
    namespace
    {
        constexpr size_t coderBufferSize = 64 * 1024;
    }

    AlsaPorts::AlsaPorts( const std::string& clientName, const std::string& inputName, const std::string& outputName )
    {
        if( snd_seq_open( &this->seq, "default", SND_SEQ_OPEN_DUPLEX, 0 ) < 0 )
        {
            log( "Cannot open the ALSA sequencer" );
            this->seq = nullptr;
            return;
        }
        snd_seq_set_client_name( this->seq, clientName.c_str() );

        // The input of the DAW: it reads the device's MIDI from here
        this->inputPort = snd_seq_create_simple_port( this->seq, inputName.c_str(), SND_SEQ_PORT_CAP_READ | SND_SEQ_PORT_CAP_SUBS_READ,
                                                      SND_SEQ_PORT_TYPE_MIDI_GENERIC | SND_SEQ_PORT_TYPE_HARDWARE | SND_SEQ_PORT_TYPE_PORT );
        // The output of the DAW: LEDs and displays
        this->outputPort = snd_seq_create_simple_port( this->seq, outputName.c_str(), SND_SEQ_PORT_CAP_WRITE | SND_SEQ_PORT_CAP_SUBS_WRITE,
                                                       SND_SEQ_PORT_TYPE_MIDI_GENERIC | SND_SEQ_PORT_TYPE_HARDWARE | SND_SEQ_PORT_TYPE_PORT );
        if( this->inputPort < 0 || this->outputPort < 0 || snd_midi_event_new( coderBufferSize, &this->encoder ) < 0
            || snd_midi_event_new( coderBufferSize, &this->decoder ) < 0 )
        {
            log( "Cannot create the ALSA ports of " + clientName );
            return;
        }
        snd_midi_event_no_status( this->decoder, 1 );
        log( "ALSA ports '" + inputName + "' and '" + outputName + "' (client " + std::to_string( snd_seq_client_id( this->seq ) ) + ")" );
    }

    AlsaPorts::~AlsaPorts()
    {
        if( this->encoder != nullptr )
            snd_midi_event_free( this->encoder );
        if( this->decoder != nullptr )
            snd_midi_event_free( this->decoder );
        if( this->seq != nullptr )
            snd_seq_close( this->seq );
    }

    void AlsaPorts::send( const uint8_t* data, size_t length )
    {
        if( this->seq == nullptr || this->encoder == nullptr )
            return;
        std::lock_guard<std::mutex> lock( this->sendLock );
        snd_midi_event_reset_encode( this->encoder );
        size_t offset = 0;
        while( offset < length )
        {
            snd_seq_event_t event;
            snd_seq_ev_clear( &event );
            const long used = snd_midi_event_encode( this->encoder, data + offset, (long)( length - offset ), &event );
            if( used <= 0 )
                break;
            offset += (size_t)used;
            if( event.type == SND_SEQ_EVENT_NONE )
                continue;
            snd_seq_ev_set_source( &event, this->inputPort );
            snd_seq_ev_set_subs( &event );
            snd_seq_ev_set_direct( &event );
            snd_seq_event_output_direct( this->seq, &event );
        }
    }

    void AlsaPorts::receive( int timeoutMilliseconds, const Receiver& receiver )
    {
        if( this->seq == nullptr || this->decoder == nullptr )
            return;

        const int count = snd_seq_poll_descriptors_count( this->seq, POLLIN );
        std::vector<pollfd> descriptors( (size_t)count );
        snd_seq_poll_descriptors( this->seq, descriptors.data(), (unsigned)count, POLLIN );
        if( ::poll( descriptors.data(), descriptors.size(), timeoutMilliseconds ) <= 0 )
            return;

        snd_seq_event_t* event = nullptr;
        while( snd_seq_event_input( this->seq, &event ) >= 0 && event != nullptr )
        {
            if( event->type == SND_SEQ_EVENT_SYSEX )
            {
                // A sysex can come in several events
                const auto* bytes = (const uint8_t*)event->data.ext.ptr;
                this->sysex.insert( this->sysex.end(), bytes, bytes + event->data.ext.len );
                if( !this->sysex.empty() && this->sysex.back() == 0xF7 )
                {
                    if( this->sysex.front() == 0xF0 )
                        receiver( this->sysex.data(), this->sysex.size() );
                    this->sysex.clear();
                }
            }
            else
            {
                uint8_t    buffer[16];
                const long length = snd_midi_event_decode( this->decoder, buffer, sizeof buffer, event );
                if( length > 0 )
                    receiver( buffer, (size_t)length );
            }
            if( snd_seq_event_input_pending( this->seq, 0 ) <= 0 )
                break;
        }
    }
}
