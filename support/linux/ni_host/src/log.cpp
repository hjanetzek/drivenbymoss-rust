// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#include "log.h"

#include <cstdio>
#include <mutex>

namespace dbm::nihost
{
    namespace
    {
        std::mutex  logLock;
        LogFunction logFunction;
    }

    void setLogFunction( LogFunction function )
    {
        std::lock_guard<std::mutex> lock( logLock );
        logFunction = std::move( function );
    }

    void log( const std::string& message )
    {
        std::lock_guard<std::mutex> lock( logLock );
        if( logFunction )
            logFunction( message );
        else
            std::fprintf( stderr, "[ni_host] %s\n", message.c_str() );
    }
}
