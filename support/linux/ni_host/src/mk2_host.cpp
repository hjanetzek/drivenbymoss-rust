// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#include "mk2_host.h"

#include "palette.h"
#include "template_map.h"

#include <algorithm>
#include <iterator>
#include <stdexcept>
#include <string>

namespace dbm::nihost
{
    namespace
    {
        using B = MaschineMk2::Button;
        using L = MaschineMk2::Led;

        struct ButtonInfo
        {
            int         button;
            const char* id; // nullptr: not in the template
            int         led;
        };

        const ButtonInfo buttonInfos[] = {
            { B::Control, "Control", L::LedControl },
            { B::Step, "Step", L::LedStep },
            { B::Browse, "Browse", L::LedBrowse },
            { B::Sampling, "Sampling", L::LedSampling },
            { B::BrowseLeft, nullptr, L::LedBrowseLeft },
            { B::BrowseRight, nullptr, L::LedBrowseRight },
            { B::All, "All", L::LedAll },
            { B::AutoWrite, "AutoWrite", L::LedAutoWrite },
            { B::Volume, "Volume", L::LedVolume },
            { B::Swing, "Swing", L::LedSwing },
            { B::Tempo, "Tempo", L::LedTempo },
            { B::MasterLeft, "MasterL", L::LedMasterLeft },
            { B::MasterRight, "MasterR", L::LedMasterRight },
            { B::Enter, "Enter", L::LedEnter },
            { B::NoteRepeat, "NoteRep", L::LedNoteRepeat },
            { B::Main, "Push", L::NoLed },
            { B::Restart, "Restart", L::LedRestart },
            { B::TransportLeft, "StepL", L::LedTransportLeft },
            { B::TransportRight, "StepR", L::LedTransportRight },
            { B::Grid, "Grid", L::LedGrid },
            { B::Play, "Play", L::LedPlay },
            { B::Rec, "Rec", L::LedRec },
            { B::Erase, "Erase", L::LedErase },
            { B::Scene, "Scene", L::LedScene },
            { B::Pattern, "Pattern", L::LedPattern },
            { B::PadMode, "PadMode", L::LedPadMode },
            { B::Navigate, "Navigate", L::LedNavigate },
            { B::Duplicate, "Duplicate", L::LedDuplicate },
            { B::Select, "Select", L::LedSelect },
            { B::Solo, "Solo", L::LedSolo },
            { B::Mute, "Mute", L::LedMute },
        };

        const TemplateControl& control( const std::string& id, Direction direction = Direction::Input )
        {
            const TemplateControl* c = findControl( templates::mk2, id.c_str(), direction );
            if( c == nullptr )
                throw std::logic_error( "The Mk2 template has no control " + id );
            return *c;
        }

        int monoLed( int button )
        {
            if( button >= B::Display1 && button < B::Display1 + 8 )
                return L::LedDisplay1 + button - B::Display1;
            for( const ButtonInfo& info : buttonInfos )
                if( info.button == button )
                    return info.led;
            return L::NoLed;
        }

        // NI numbers the pads from the bottom left, MaschineMk2 from the top left
        int niPadNumber( int index )
        {
            const int row = index / 4, column = index % 4;
            return ( 3 - row ) * 4 + column + 1;
        }

        uint8_t toLed( uint8_t value ) { return (uint8_t)( value >> 1 ); } // 0..255 -> 0..0x7F

        // The messages of DrivenByMoss for dbm-nihost (not in NI's driver): F0 7D 44 42 4D <id> ...
        // 11: the query of DrivenByMoss (F7) and the answer (01 F7): the Shift button is sent
        // 12: the Shift button, 01 down, 00 up
        constexpr uint8_t dbmHeader[] = { 0xF0, 0x7D, 0x44, 0x42, 0x4D };
        constexpr uint8_t dbmHost = 0x11, dbmShift = 0x12;
    }


    Mk2Host::Mk2Host( MidiOut midiOut ) : out( std::move( midiOut ) )
    {
        for( const ButtonInfo& info : buttonInfos )
            if( info.id != nullptr )
                this->buttonControls[(size_t)info.button] = &control( info.id );
        for( int i = 0; i < 8; i++ )
        {
            this->buttonControls[(size_t)( B::Display1 + i )] = &control( "Button" + std::to_string( i + 1 ) );
            const std::string group                          = std::string( "Group" ) + (char)( 'A' + i );
            this->buttonControls[(size_t)( B::GroupA + i )]   = &control( group );
            this->ccButtons[control( group + "H", Direction::Led ).number] = B::GroupA + i;
            this->knobs[(size_t)i] = &control( "Knob" + std::to_string( i + 1 ) );
        }
        for( int i = 0; i < MaschineMk2::numPads; i++ )
        {
            const std::string pad = "Pad" + std::to_string( niPadNumber( i ) );
            this->pads[(size_t)i] = &control( pad );
            this->notePads[control( pad + "H", Direction::Led ).number] = i;
        }
        // The mono LEDs are set with the message of their button
        for( int button = 0; button < MaschineMk2::NumButtons; button++ )
        {
            const TemplateControl* c = this->buttonControls[(size_t)button];
            if( c != nullptr && c->kind == MessageKind::Cc && monoLed( button ) != L::NoLed )
                this->ccButtons[c->number] = button;
        }
        this->dial = &control( "Dial" );
    }


    void Mk2Host::attached()
    {
        this->mk2.reset();
        this->encoderRemainder = {};
        // force a redraw of the displays
        this->text.setText( 0, this->text.line( 0 ) );
        this->lastDisplayUpdate = {};
        MaschineMk2::Display left{}, right{};
        const auto           l = this->text.render( 0 ), r = this->text.render( 1 );
        std::copy( l.begin(), l.end(), left.begin() );
        std::copy( r.begin(), r.end(), right.begin() );
        this->mk2.setDisplay( 0, left );
        this->mk2.setDisplay( 1, right );
        this->announceShift();
    }


    void Mk2Host::dawConnected()
    {
        this->announceShift();
    }


    void Mk2Host::announceShift()
    {
        uint8_t message[sizeof dbmHeader + 3];
        std::copy( std::begin( dbmHeader ), std::end( dbmHeader ), message );
        message[sizeof dbmHeader]     = dbmHost;
        message[sizeof dbmHeader + 1] = 0x01;
        message[sizeof dbmHeader + 2] = 0xF7;
        this->out( message, sizeof message );
    }


    void Mk2Host::hidReport( const uint8_t* report, size_t length )
    {
        this->mk2.decode( report, length, *this );
    }


    void Mk2Host::flush( const ReportWriter& write )
    {
        const auto now = std::chrono::steady_clock::now();
        if( now - this->lastDisplayUpdate >= std::chrono::milliseconds( 50 ) && this->text.takeChanged() )
        {
            this->lastDisplayUpdate = now;
            for( int d = 0; d < 2; d++ )
            {
                const auto           bitmap = this->text.render( d );
                MaschineMk2::Display pixels{};
                std::copy( bitmap.begin(), bitmap.end(), pixels.begin() );
                this->mk2.setDisplay( d, pixels );
            }
        }
        this->mk2.flush( write );
    }


    void Mk2Host::mk2Button( int button, bool down )
    {
        if( button == B::Shift )
        {
            // Not in the template (DrivenByMoss emulates Shift with STEP LEFT on the Mk2 with NI's
            // driver): light it and send it as a DrivenByMoss sysex
            this->mk2.setLed( L::LedShift, down ? MaschineMk2::lightOn : MaschineMk2::lightOff );
            uint8_t message[sizeof dbmHeader + 3];
            std::copy( std::begin( dbmHeader ), std::end( dbmHeader ), message );
            message[sizeof dbmHeader]     = dbmShift;
            message[sizeof dbmHeader + 1] = down ? 0x01 : 0x00;
            message[sizeof dbmHeader + 2] = 0xF7;
            this->out( message, sizeof message );
            return;
        }
        if( button < 0 || button >= MaschineMk2::NumButtons )
            return;
        const TemplateControl* c = this->buttonControls[(size_t)button];
        if( c != nullptr )
            sendControl( this->out, *c, down ? c->on : c->off );
    }


    void Mk2Host::mk2MainEncoder( int steps )
    {
        sendControl( this->out, *this->dial, twosComplement( steps ) );
    }


    void Mk2Host::mk2Encoder( int encoder, int delta )
    {
        if( encoder < 0 || encoder >= MaschineMk2::numEncoders )
            return;
        // counts -> steps, keeping the remainder
        int& remainder = this->encoderRemainder[(size_t)encoder];
        remainder += delta * stepsPerTurn;
        const int steps = remainder / encoderCountsPerTurn;
        remainder -= steps * encoderCountsPerTurn;
        if( steps != 0 )
            sendControl( this->out, *this->knobs[(size_t)encoder], twosComplement( steps ) );
    }


    void Mk2Host::mk2Pad( int pad, bool down, int velocity )
    {
        if( pad < 0 || pad >= MaschineMk2::numPads )
            return;
        sendControl( this->out, *this->pads[(size_t)pad], down ? velocity : 0 );
    }


    void Mk2Host::midiIn( const uint8_t* data, size_t length )
    {
        if( length == 0 )
            return;
        if( data[0] == 0xF0 )
        {
            // The query of DrivenByMoss whether the Shift button is sent
            if( length == sizeof dbmHeader + 2 && std::equal( std::begin( dbmHeader ), std::end( dbmHeader ), data ) && data[sizeof dbmHeader] == dbmHost
                && data[sizeof dbmHeader + 1] == 0xF7 )
            {
                this->announceShift();
                return;
            }
            this->text.handleSysex( data, length );
            return;
        }
        if( length < 3 )
            return;

        const int status = data[0] & 0xF0, channel = data[0] & 0x0F, number = data[1], value = data[2];
        if( status == 0xB0 && channel == 0 )
        {
            const auto it = this->ccButtons.find( number );
            if( it != this->ccButtons.end() )
                this->setButtonLed( it->second, value );
        }
        else if( ( status == 0x90 || status == 0x80 ) && channel == 0 )
        {
            // The 7th display button is an MCU button, lit with its note
            if( number == this->buttonControls[(size_t)( B::Display1 + 6 )]->number )
            {
                this->mk2.setLed( L::LedDisplay1 + 6, status == 0x80 || value == 0 ? MaschineMk2::lightOff : MaschineMk2::lightOn );
                return;
            }
            const auto it = this->notePads.find( number );
            if( it != this->notePads.end() )
            {
                const Rgb c = paletteColour( status == 0x80 ? 0 : value );
                this->mk2.setPad( it->second, toLed( c.r ), toLed( c.g ), toLed( c.b ) );
            }
        }
    }


    void Mk2Host::setButtonLed( int button, int value )
    {
        if( button >= B::GroupA && button < B::GroupA + 8 )
        {
            const Rgb c = paletteColour( value );
            this->mk2.setGroup( button - B::GroupA, toLed( c.r ), toLed( c.g ), toLed( c.b ) );
            return;
        }
        const int led = monoLed( button );
        if( led != L::NoLed )
            this->mk2.setLed( led, (uint8_t)( value > 0x7F ? 0x7F : value ) );
    }
}
