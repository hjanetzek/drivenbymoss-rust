// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#pragma once

#include <functional>
#include <string>

namespace dbm::nihost
{
    // Where the library logs to; stderr by default. Set it before creating devices.
    using LogFunction = std::function<void( const std::string& )>;
    void setLogFunction( LogFunction function );
    void log( const std::string& message );
}
