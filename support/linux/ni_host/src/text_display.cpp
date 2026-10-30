// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#include "text_display.h"

#include "font.h"

#include <algorithm>

namespace dbm::nihost
{
    namespace
    {
        // Mackie Control display: F0 00 00 66 <device> 12 <position> <characters> F7
        constexpr uint8_t mcuHeader[] = { 0xF0, 0x00, 0x00, 0x66 };
        constexpr uint8_t mcuDisplayCommand = 0x12;
        // The labels of the buttons above the displays: F0 7D 44 42 4D 10 <position> <characters> F7
        constexpr uint8_t labelsHeader[] = { 0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x10 };
        // The labels are drawn at single height in the free rows above the first line
        constexpr int labelsY = 2;

        // A character cell is 9 pixels wide (28 x 9 = 252 of 256), the lines start at y = 12 and 40
        constexpr int cellWidth = 9, firstX = 2;
        constexpr int lineY[McuTextDisplay::lines] = { 12, 40 };
    }

    McuTextDisplay::McuTextDisplay()
    {
        this->text.fill( ' ' );
        this->buttonLabels.fill( ' ' );
    }

    bool McuTextDisplay::handleSysex( const uint8_t* data, size_t length )
    {
        // header, position, at least one character, F7
        if( length >= sizeof labelsHeader + 3 && std::equal( std::begin( labelsHeader ), std::end( labelsHeader ), data ) )
        {
            const size_t position = data[sizeof labelsHeader];
            for( size_t i = 0; i < length - sizeof labelsHeader - 2; i++ )
            {
                const size_t index = position + i;
                if( index >= this->buttonLabels.size() )
                    break;
                const char raw = (char)data[sizeof labelsHeader + 1 + i];
                const char c   = raw >= 32 && raw < 127 ? raw : ' ';
                if( this->buttonLabels[index] != c )
                {
                    this->buttonLabels[index] = c;
                    this->changed             = true;
                }
            }
            return true;
        }

        // header, device id, command, position, at least one character, F7
        if( length < sizeof mcuHeader + 4 || !std::equal( std::begin( mcuHeader ), std::end( mcuHeader ), data ) )
            return false;
        if( data[sizeof mcuHeader + 1] != mcuDisplayCommand )
            return false;
        const int position = data[sizeof mcuHeader + 2];
        std::string characters( (const char*)data + sizeof mcuHeader + 3, length - sizeof mcuHeader - 4 );
        this->setText( position, characters );
        return true;
    }

    void McuTextDisplay::setText( int position, const std::string& characters )
    {
        for( size_t i = 0; i < characters.size(); i++ )
        {
            const size_t index = (size_t)position + i;
            if( index >= this->text.size() )
                break;
            const char c = characters[i] >= 32 && characters[i] < 127 ? characters[i] : ' ';
            if( this->text[index] != c )
            {
                this->text[index] = c;
                this->changed     = true;
            }
        }
    }

    std::string McuTextDisplay::labels() const
    {
        return std::string( this->buttonLabels.data(), lineLength );
    }

    std::string McuTextDisplay::line( int index ) const
    {
        if( index < 0 || index >= lines )
            return {};
        return std::string( this->text.data() + index * lineLength, lineLength );
    }

    McuTextDisplay::Bitmap McuTextDisplay::render( int display ) const
    {
        Bitmap bitmap{};
        const auto setPixel = [&bitmap]( int x, int y ) {
            if( x < 0 || x >= (int)displayWidth || y < 0 || y >= (int)displayHeight )
                return;
            bitmap[(size_t)( y * (int)displayWidth / 8 + x / 8 )] |= (uint8_t)( 0x80 >> ( x % 8 ) );
        };

        constexpr int charsPerDisplay = lineLength / 2;
        for( int column = 0; column < charsPerDisplay; column++ )
        {
            const char c = this->buttonLabels[(size_t)( display * charsPerDisplay + column )];
            if( c < fontFirstChar || c > fontLastChar )
                continue;
            const auto& glyph = font5x7[c - fontFirstChar];
            for( int y = 0; y < fontHeight; y++ )
                for( int x = 0; x < fontWidth; x++ )
                    if( glyph[y] & ( 0x80 >> x ) )
                        setPixel( firstX + column * cellWidth + x, labelsY + y );
        }
        for( int row = 0; row < lines; row++ )
        {
            for( int column = 0; column < charsPerDisplay; column++ )
            {
                const char c = this->text[(size_t)( row * lineLength + display * charsPerDisplay + column )];
                if( c < fontFirstChar || c > fontLastChar )
                    continue;
                const auto& glyph = font5x7[c - fontFirstChar];
                const int   x0    = firstX + column * cellWidth;
                for( int y = 0; y < fontHeight; y++ )
                    for( int x = 0; x < fontWidth; x++ )
                        if( glyph[y] & ( 0x80 >> x ) )
                        {
                            // drawn at double height for readability
                            setPixel( x0 + x, lineY[row] + 2 * y );
                            setPixel( x0 + x, lineY[row] + 2 * y + 1 );
                        }
            }
        }
        return bitmap;
    }

    bool McuTextDisplay::takeChanged()
    {
        const bool result = this->changed;
        this->changed     = false;
        return result;
    }
}
