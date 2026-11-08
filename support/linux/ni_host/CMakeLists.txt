# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
#
# NI host emulation for Linux: drives the Maschine Mk2 and JAM over hidraw and translates their
# HID reports to the MIDI of the DrivenByMoss NI templates (see doc/NI_HOST_LINUX_PLAN.md).

cmake_minimum_required(VERSION 3.15)
project(dbm_ni_host VERSION 0.2.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release)
endif()

set(NIHOST_SOURCES
    src/log.cpp
    src/hidraw_device.cpp
    src/maschine_jam.cpp
    src/maschine_mk2.cpp
    src/palette.cpp
    src/text_display.cpp
    src/jam_host.cpp
    src/mk2_host.cpp
)
add_library(dbm_ni_host STATIC ${NIHOST_SOURCES})
target_include_directories(dbm_ni_host PUBLIC src include)
target_compile_options(dbm_ni_host PRIVATE -Wall -Wextra)
find_package(Threads REQUIRED)
target_link_libraries(dbm_ni_host PUBLIC Threads::Threads)
set_target_properties(dbm_ni_host PROPERTIES POSITION_INDEPENDENT_CODE ON)

# Logs the decoded events of the connected devices and lights a test pattern
add_executable(nihost-probe tools/probe.cpp)
target_link_libraries(nihost-probe dbm_ni_host)
target_compile_options(nihost-probe PRIVATE -Wall -Wextra)

# The host as a shared library with a C interface (include/dbm_nihost.h), for a process which
# drives the devices itself: the DrivenByMoss zynthian drivers load it with ctypes. No ALSA.
# Only the C functions are exported; loaded by its path, hence no soname version.
add_library(dbm_nihost SHARED ${NIHOST_SOURCES} src/c_api.cpp)
target_include_directories(dbm_nihost PRIVATE src include)
target_compile_options(dbm_nihost PRIVATE -Wall -Wextra)
target_link_libraries(dbm_nihost PRIVATE Threads::Threads)
set_target_properties(dbm_nihost PROPERTIES CXX_VISIBILITY_PRESET hidden VISIBILITY_INLINES_HIDDEN YES)

# The daemon: ALSA sequencer ports like NI's driver creates them (needs the ALSA headers; without
# them only the library and the tools are built)
find_package(PkgConfig)
if(PkgConfig_FOUND)
    pkg_check_modules(ALSA IMPORTED_TARGET alsa)
endif()
if(ALSA_FOUND)
    add_executable(dbm-nihost src/alsa_ports.cpp src/daemon.cpp)
    target_link_libraries(dbm-nihost dbm_ni_host PkgConfig::ALSA)
    target_compile_options(dbm-nihost PRIVATE -Wall -Wextra)
else()
    message(WARNING "ALSA (alsa-lib) not found: dbm-nihost (the daemon) is not built")
endif()

# Installation: the daemon, the systemd user service and the system files (udev rule, snd-virmidi
# setup), which have to be copied to /etc by hand when not installing to /usr
include(GNUInstallDirs)
if(ALSA_FOUND)
    install(TARGETS dbm-nihost RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
endif()
# The library in a folder of its own (the zynthian drivers look for
# /usr/lib/dbm-nihost/libdbm_nihost.so), with its header
install(TARGETS dbm_nihost LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}/dbm-nihost)
install(FILES include/dbm_nihost.h DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
configure_file(packaging/dbm-nihost.service.in dbm-nihost.service @ONLY)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/dbm-nihost.service DESTINATION ${CMAKE_INSTALL_DATADIR}/systemd/user)
install(FILES packaging/70-dbm-nihost.rules packaging/modprobe-dbm-nihost.conf packaging/modules-load-dbm-nihost.conf
        DESTINATION ${CMAKE_INSTALL_DATADIR}/dbm-nihost)
# Debugging helpers (Python, need the module "jack"): the ports as zynthian sees them through
# PipeWire's MIDI bridge
install(PROGRAMS tools/dbm-nihost-jack-probe tools/dbm-nihost-bridge-test
        DESTINATION ${CMAKE_INSTALL_DATADIR}/dbm-nihost/tools)

enable_testing()
add_executable(nihost-test tests/host_test.cpp)
target_link_libraries(nihost-test dbm_ni_host)
target_compile_options(nihost-test PRIVATE -Wall -Wextra)
add_test(NAME nihost-test COMMAND nihost-test)
add_executable(nihost-c-api-test tests/c_api_test.cpp)
target_include_directories(nihost-c-api-test PRIVATE include src)
target_link_libraries(nihost-c-api-test dbm_nihost)
target_compile_options(nihost-c-api-test PRIVATE -Wall -Wextra)
add_test(NAME nihost-c-api-test COMMAND nihost-c-api-test)
