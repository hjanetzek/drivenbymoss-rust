# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
#
# NI host emulation for Linux: drives the Maschine Mk2 and JAM over hidraw and translates their
# HID reports to the MIDI of the DrivenByMoss NI templates (see doc/NI_HOST_LINUX_PLAN.md).

cmake_minimum_required(VERSION 3.15)
project(dbm_ni_host VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release)
endif()

add_library(dbm_ni_host STATIC
    src/log.cpp
    src/hidraw_device.cpp
    src/maschine_jam.cpp
    src/maschine_mk2.cpp
    src/palette.cpp
    src/text_display.cpp
    src/jam_host.cpp
    src/mk2_host.cpp
)
target_include_directories(dbm_ni_host PUBLIC src include)
target_compile_options(dbm_ni_host PRIVATE -Wall -Wextra)
find_package(Threads REQUIRED)
target_link_libraries(dbm_ni_host PUBLIC Threads::Threads)
set_target_properties(dbm_ni_host PROPERTIES POSITION_INDEPENDENT_CODE ON)

# Logs the decoded events of the connected devices and lights a test pattern
add_executable(nihost-probe tools/probe.cpp)
target_link_libraries(nihost-probe dbm_ni_host)
target_compile_options(nihost-probe PRIVATE -Wall -Wextra)

# The daemon: ALSA sequencer ports like NI's driver creates them
find_package(PkgConfig REQUIRED)
pkg_check_modules(ALSA REQUIRED IMPORTED_TARGET alsa)
add_executable(dbm-nihost src/alsa_ports.cpp src/daemon.cpp)
target_link_libraries(dbm-nihost dbm_ni_host PkgConfig::ALSA)
target_compile_options(dbm-nihost PRIVATE -Wall -Wextra)

# Installation: the daemon, the systemd user service and the system files (udev rule, snd-virmidi
# setup), which have to be copied to /etc by hand when not installing to /usr
include(GNUInstallDirs)
install(TARGETS dbm-nihost RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
configure_file(packaging/dbm-nihost.service.in dbm-nihost.service @ONLY)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/dbm-nihost.service DESTINATION ${CMAKE_INSTALL_DATADIR}/systemd/user)
install(FILES packaging/70-dbm-nihost.rules packaging/modprobe-dbm-nihost.conf packaging/modules-load-dbm-nihost.conf
        DESTINATION ${CMAKE_INSTALL_DATADIR}/dbm-nihost)

enable_testing()
add_executable(nihost-test tests/host_test.cpp)
target_link_libraries(nihost-test dbm_ni_host)
target_compile_options(nihost-test PRIVATE -Wall -Wextra)
add_test(NAME nihost-test COMMAND nihost-test)
