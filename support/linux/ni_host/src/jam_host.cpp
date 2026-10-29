// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#include "jam_host.h"

#include "template_map.h"

#include <algorithm>
#include <stdexcept>
#include <string>

namespace dbm::nihost
{
    namespace
    {
        using B = MaschineJam::Button;
        using L = MaschineJam::MonoLed;

        // The template id and mono LED of each button (except pads, groups and scenes)
        struct ButtonInfo
        {
            int         button;
            const char* id;
            int         led;
        };

        const ButtonInfo buttonInfos[] = {
            { B::Song, "BtnArrange", L::LedSong },
            { B::Step, "BtnStep", L::LedStep },
            { B::PadMode, "BtnPadMode", L::LedPadMode },
            { B::Clear, "BtnClear", L::LedClear },
            { B::Duplicate, "BtnDuplicate", L::LedDuplicate },
            { B::Up, "BtnDpad1", L::LedUp },
            { B::Right, "BtnDpad2", L::LedRight },
            { B::Down, "BtnDpad3", L::LedDown },
            { B::Left, "BtnDpad4", L::LedLeft },
            { B::NoteRepeat, "BtnArpRepeat", L::LedNoteRepeat },
            { B::Master, "BtnMst", L::LedMaster },
            { B::Group, "BtnGrp", L::LedGroup },
            { B::In, "BtnIn1", L::LedIn },
            { B::Cue, "BtnCue", L::LedCue },
            { B::Browse, "BtnBrowse", L::LedBrowse },
            { B::Macro, "BtnMacro", L::LedMacro },
            { B::Level, "BtnLevel", L::LedLevel },
            { B::Aux, "BtnAux", L::LedAux },
            { B::Control, "BtnControl", L::LedControl },
            { B::Auto, "BtnAuto", L::LedAuto },
            { B::Perform, "BtnPerform", L::LedPerform },
            { B::Variation, "BtnVariation", L::LedVariation },
            { B::Lock, "BtnLock", L::LedLock },
            { B::Tune, "BtnTune", L::LedTune },
            { B::Swing, "BtnSwing", L::LedSwing },
            { B::Play, "BtnPlay", L::LedPlay },
            { B::Rec, "BtnRecord", L::LedRec },
            { B::PageLeft, "BtnArrowLeft", L::LedPageLeft },
            { B::PageRight, "BtnArrowRight", L::LedPageRight },
            { B::Tempo, "BtnTempo", L::LedTempo },
            { B::Grid, "BtnGrid", L::LedGrid },
            { B::Solo, "BtnSolo", L::LedSolo },
            { B::Mute, "BtnMute", L::LedMute },
            { B::Select, "BtnSelect", L::LedSelect },
            { B::EncoderTouch, "CapBrowse", L::NoLed },
            { B::EncoderPush, "PshBrowse", L::NoLed },
        };

        int monoLed( int button )
        {
            for( const ButtonInfo& info : buttonInfos )
                if( info.button == button )
                    return info.led;
            return L::NoLed;
        }

        // The JAM's mono LEDs are almost fully bright from about 0x30 on (checked on the device),
        // so the value is mapped quadratically: 0x7F stays full, 0x30 becomes dim (0x12)
        uint8_t monoBrightness( int value )
        {
            if( value <= 0 )
                return 0;
            if( value > 0x7F )
                value = 0x7F;
            const int brightness = value * value / 0x7F;
            return (uint8_t)( brightness < 1 ? 1 : brightness );
        }

        // NI's sysex header with the JAM id (17cc:1500) and the commands
        constexpr uint8_t sysexHeader[] = { 0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01 };
        constexpr uint8_t commandShift = 0x4D, commandReturnFromHost = 0x46, commandStripSetup = 0x05, commandStripDualValues = 0x04;
        constexpr size_t  stripValueOffset = 11;

        const TemplateControl& control( const char* id, Direction direction = Direction::Input )
        {
            const TemplateControl* c = findControl( templates::jam, id, direction );
            if( c == nullptr )
                throw std::logic_error( std::string( "The JAM template has no control " ) + id );
            return *c;
        }
    }


    JamHost::JamHost( MidiOut midiOut ) : out( std::move( midiOut ) )
    {
        for( const ButtonInfo& info : buttonInfos )
            this->buttonControls[(size_t)info.button] = &control( info.id );

        for( int i = 0; i < MaschineJam::numPads; i++ )
        {
            // Btn<column A..H><row 1..8 from the top>
            const int         x  = i % 8, y = i / 8;
            const std::string id = std::string( "Btn" ) + (char)( 'A' + x ) + std::to_string( y + 1 );
            this->buttonControls[(size_t)( B::Pad1 + i )] = &control( id.c_str() );
            this->notePads[control( ( id + "IDX" ).c_str(), Direction::Led ).number] = i;
        }

        for( int i = 0; i < 8; i++ )
        {
            const std::string group = std::string( "BtnGroup" ) + (char)( 'A' + i );
            this->buttonControls[(size_t)( B::GroupA + i )] = &control( group.c_str() );
            this->ccButtons[control( ( group + "IDX" ).c_str(), Direction::Led ).number] = B::GroupA + i;

            const std::string scene = "BtnScene" + std::to_string( i + 1 );
            this->buttonControls[(size_t)( B::Scene1 + i )] = &control( scene.c_str() );
            this->ccButtons[control( ( scene + "IDX" ).c_str(), Direction::Led ).number] = B::Scene1 + i;

            const std::string strip = std::string( "Tst" ) + (char)( 'A' + i );
            this->stripValue[(size_t)i] = &control( strip.c_str() );
            this->stripTouch[(size_t)i] = &control( ( "Cap" + strip ).c_str() );
        }

        // The mono LEDs are set with the message of their button
        for( const ButtonInfo& info : buttonInfos )
            if( info.led != L::NoLed )
                this->ccButtons[this->buttonControls[(size_t)info.button]->number] = info.button;

        this->encoder = &control( "EncBrowse" );
    }


    void JamHost::attached()
    {
        this->jam.reset();
        this->stripTouched = {};
        uint8_t message[sizeof sysexHeader + 3];
        std::copy( std::begin( sysexHeader ), std::end( sysexHeader ), message );
        message[sizeof sysexHeader]     = commandReturnFromHost;
        message[sizeof sysexHeader + 1] = 0x01;
        message[sizeof sysexHeader + 2] = 0xF7;
        this->out( message, sizeof message );
    }


    void JamHost::hidReport( const uint8_t* report, size_t length )
    {
        this->jam.decode( report, length, *this );
    }


    void JamHost::flush( const ReportWriter& write )
    {
        this->jam.flush( write );
    }


    void JamHost::jamButton( int button, bool down )
    {
        if( button == B::Shift )
        {
            // Shift is not in the template, NI's driver sends it as a sysex
            uint8_t message[sizeof sysexHeader + 3];
            std::copy( std::begin( sysexHeader ), std::end( sysexHeader ), message );
            message[sizeof sysexHeader]     = commandShift;
            message[sizeof sysexHeader + 1] = down ? 0x01 : 0x00;
            message[sizeof sysexHeader + 2] = 0xF7;
            this->out( message, sizeof message );
            this->jam.setMono( L::LedShift, down ? MaschineJam::lightOn : MaschineJam::lightOff );
            return;
        }

        if( button < 0 || button >= MaschineJam::NumButtons )
            return;
        const TemplateControl* c = this->buttonControls[(size_t)button];
        if( c != nullptr )
            sendControl( this->out, *c, down ? c->on : c->off );
    }


    void JamHost::jamEncoder( int steps )
    {
        sendControl( this->out, *this->encoder, twosComplement( steps ) );
    }


    void JamHost::jamStrip( int strip, int position )
    {
        if( strip < 0 || strip >= MaschineJam::numStrips )
            return;
        const auto index = (size_t)strip;
        if( position < 0 )
        {
            this->stripTouched[index] = false;
            sendControl( this->out, *this->stripTouch[index], this->stripTouch[index]->off );
            return;
        }
        if( !this->stripTouched[index] )
        {
            this->stripTouched[index] = true;
            sendControl( this->out, *this->stripTouch[index], this->stripTouch[index]->on );
        }
        sendControl( this->out, *this->stripValue[index], position * 127 / MaschineJam::stripMax );
    }


    void JamHost::midiIn( const uint8_t* data, size_t length )
    {
        if( length == 0 )
            return;
        if( data[0] == 0xF0 )
        {
            this->handleSysex( data, length );
            return;
        }
        if( length < 3 )
            return;

        const int status = data[0] & 0xF0, number = data[1], value = data[2];
        if( status == 0xB0 )
        {
            // touch strip values
            for( int i = 0; i < MaschineJam::numStrips; i++ )
                if( this->stripValue[(size_t)i]->number == number )
                {
                    this->strips[(size_t)i].value = value;
                    this->drawStrip( i );
                    return;
                }
            const auto it = this->ccButtons.find( number );
            if( it != this->ccButtons.end() )
                this->setButtonLed( it->second, value );
        }
        else if( status == 0x90 || status == 0x80 )
        {
            const auto it = this->notePads.find( number );
            if( it != this->notePads.end() )
                this->jam.setPad( it->second % 8, it->second / 8, status == 0x80 ? 0 : (uint8_t)value );
        }
    }


    void JamHost::setButtonLed( int button, int value )
    {
        // The RGB LEDs take the value as index of NI's palette (hue * 4 + level). The JAM's HID
        // palette is the same (checked on the device for 0..79: 64..67 light red, 68..71 white,
        // 78/79 white), so the value is passed through
        if( button >= B::GroupA && button < B::GroupA + 8 )
            this->jam.setGroup( button - B::GroupA, (uint8_t)value );
        else if( button >= B::Scene1 && button < B::Scene1 + 8 )
            this->jam.setScene( button - B::Scene1, (uint8_t)value );
        else
        {
            const int led = monoLed( button );
            if( led != L::NoLed )
                this->jam.setMono( led, monoBrightness( value ) );
        }
    }


    void JamHost::handleSysex( const uint8_t* data, size_t length )
    {
        if( length < sizeof sysexHeader + 2 || !std::equal( std::begin( sysexHeader ), std::end( sysexHeader ), data ) )
            return;
        const uint8_t command = data[sizeof sysexHeader];
        if( command == commandStripSetup )
        {
            // type and colour of each strip
            for( size_t i = 0; i < MaschineJam::numStrips && stripValueOffset + 2 * i + 1 < length - 1; i++ )
            {
                this->strips[i].type   = data[stripValueOffset + 2 * i];
                this->strips[i].colour = data[stripValueOffset + 2 * i + 1];
                this->drawStrip( (int)i );
            }
        }
        else if( command == commandStripDualValues )
        {
            for( size_t i = 0; i < MaschineJam::numStrips && stripValueOffset + i < length - 1; i++ )
            {
                this->strips[i].dualValue = data[stripValueOffset + i];
                this->drawStrip( (int)i );
            }
        }
    }


    void JamHost::drawStrip( int strip )
    {
        const Strip&  s      = this->strips[(size_t)strip];
        const int     leds   = MaschineJam::stripLeds;
        const uint8_t colour = (uint8_t)s.colour;
        // the LED of a value 0..127, 0..leds-1
        const auto ledOf = []( int value ) { return value * ( MaschineJam::stripLeds - 1 ) / 127; };

        switch( s.type )
        {
            case StripDot:
                for( int i = 0; i < leds; i++ )
                    this->jam.setStripLed( strip, i, i == ledOf( s.value ) ? colour : 0 );
                break;

            case StripPan:
            {
                // a bar from the centre to the value
                const int centre = leds / 2, led = ledOf( s.value );
                for( int i = 0; i < leds; i++ )
                {
                    const bool on = ( i >= centre && i <= led ) || ( i <= centre && i >= led );
                    this->jam.setStripLed( strip, i, on ? colour : 0 );
                }
                break;
            }

            case StripDual:
            {
                // a bar for the value and a dot for the second value
                const int bar = ( s.value * leds + 63 ) / 127, dot = ledOf( s.dualValue );
                for( int i = 0; i < leds; i++ )
                    this->jam.setStripLed( strip, i, i == dot ? MaschineJam::colour( MaschineJam::HueWhite, MaschineJam::LevelFull ) : ( i < bar ? colour : 0 ) );
                break;
            }

            case StripSingle:
            default:
                this->jam.setStripBar( strip, ( s.value * leds + 63 ) / 127, colour );
                break;
        }
    }
}
