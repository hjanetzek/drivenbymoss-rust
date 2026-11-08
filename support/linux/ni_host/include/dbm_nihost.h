/* Copyright (c) 2026 Hannes Janetzek
 * Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
 *
 * libdbm_nihost: the NI host emulation of dbm-nihost as a library, for a process which drives a
 * Maschine itself (the DrivenByMoss zynthian drivers, through ctypes) instead of going through
 * dbm-nihost's ALSA sequencer ports. The MIDI is the same as on those ports: the template MIDI
 * of the DrivenByMoss NI Controller Editor templates and the DrivenByMoss sysex messages
 * (src/midi.h): the query F0 7D 44 42 4D 11 F7, the Mk2's hello F0 7D 44 42 4D 11 01 F7, the
 * JAM's hello F0 00 21 09 15 00 4D 50 00 01 46 01 F7 (NI's "return from host") and "gone"
 * F0 7D 44 42 4D 13 F7.
 *
 * A device has a thread of its own which looks for the hidraw device once a second, opens it
 * and translates its input reports (so it can be plugged in and out, and a node which is
 * briefly "Permission denied" after a re-enumeration is opened a second later). The caller
 * - reads the MIDI to the DAW with dbm_nihost_read (it may block on its own thread),
 * - passes the DAW's MIDI to dbm_nihost_send, one message per call,
 * - writes the changed LEDs and displays with dbm_nihost_flush, e.g. after each tick.
 * There are no callbacks. All functions are thread safe; dbm_nihost_close must be the last call
 * on a device, after the other threads are done with it.
 *
 * Only one process can drive a device: the library takes an exclusive flock on the hidraw node
 * (as dbm-nihost does); while another process holds it the device counts as absent.
 * Needs read and write access to /dev/hidraw* (packaging/70-dbm-nihost.rules).
 */

#ifndef DBM_NIHOST_H
#define DBM_NIHOST_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define DBM_NIHOST_API_VERSION 1

/* The devices */
#define DBM_NIHOST_MK2 1 /* Maschine Mk2, 17cc:1140 */
#define DBM_NIHOST_JAM 2 /* Maschine JAM, 17cc:1500 */

typedef struct dbm_nihost dbm_nihost;

/* DBM_NIHOST_API_VERSION of the library */
int dbm_nihost_api_version( void );

/* Start driving the device of the kind (DBM_NIHOST_MK2, DBM_NIHOST_JAM); NULL for an unknown
 * kind. The device need not be connected: once it is, the hello is read. */
dbm_nihost* dbm_nihost_open( int kind );

/* A device without hidraw, for tests: dbm_nihost_test_* stand in for the hardware. */
dbm_nihost* dbm_nihost_open_test( int kind );

/* Stops the thread and frees the device. */
void dbm_nihost_close( dbm_nihost* device );

/* 1 while the device is attached (its hidraw node is open), else 0. */
int dbm_nihost_is_attached( dbm_nihost* device );

/* The MIDI to the DAW: waits up to timeout_ms (0: no wait) for a message, then copies as many of
 * the waiting messages as fit, each as 2 bytes length (little endian) and the message.
 * Returns the number of bytes copied, 0 if none came; -1 if the buffer can't take the next
 * message (2 bytes plus the message; the longest are the Mk2's label sysex, 1024 bytes are
 * plenty).
 * The queue keeps the last 4096 messages for a caller which doesn't read. */
int dbm_nihost_read( dbm_nihost* device, uint8_t* buffer, size_t size, int timeout_ms );

/* One MIDI message from the DAW (a short message or a complete sysex): LEDs, colours, the
 * displays, the query. An answer to the query is in the next dbm_nihost_read. Returns 0. */
int dbm_nihost_send( dbm_nihost* device, const uint8_t* data, size_t length );

/* Write the changed LED reports and (at most 20 times a second) the displays to the device.
 * Returns the number of reports written (0 while the device isn't attached), -1 if a write
 * failed. */
int dbm_nihost_flush( dbm_nihost* device );

/* Tests (devices of dbm_nihost_open_test only): */
/* The device is attached (1, the host sends its hello) or unplugged (0, "gone"). */
void dbm_nihost_test_attach( dbm_nihost* device, int attached );
/* An input report of the device. */
void dbm_nihost_test_report( dbm_nihost* device, const uint8_t* report, size_t length );
/* The oldest output report written by dbm_nihost_flush: its length (0: none left), -1 if the
 * buffer is too small. */
int dbm_nihost_test_take_report( dbm_nihost* device, uint8_t* buffer, size_t size );

#ifdef __cplusplus
}
#endif

#endif
