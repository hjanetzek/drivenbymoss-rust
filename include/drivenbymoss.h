/*
 * Written by Jürgen Moßgraber - mossgrabers.de
 * (c) 2017-2026
 * Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
 *
 * C interface of the Rust port of DrivenByMoss4Reaper. It replaces the JNI interface: the
 * dbm_* functions correspond to the static methods of the former Java class
 * de.mossgrabers.reaper.Controller, the callbacks correspond to the former native methods of
 * de.mossgrabers.reaper.MainApp.
 *
 * All functions may be called from any thread; the library serializes the calls. The callbacks
 * are called from the calling thread or from the internal flush timer thread.
 */

#ifndef DRIVENBYMOSS_H
#define DRIVENBYMOSS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Adds one MIDI device to the list given to get_midi_inputs/get_midi_outputs. */
typedef void (*DbmAddDevice) (void *list, int id, const char *name);

typedef struct DbmCallbacks
{
    void *user; /* Passed as the first argument to all callbacks */

    void (*process_no_arg) (void *user, const char *processor, const char *command);
    void (*process_string_arg) (void *user, const char *processor, const char *command, const char *value);
    void (*process_string_args) (void *user, const char *processor, const char *command, const char *const *values, int count);
    void (*process_int_arg) (void *user, const char *processor, const char *command, int value);
    void (*process_double_arg) (void *user, const char *processor, const char *command, double value);
    void (*delay_updates) (void *user, const char *processor);
    void (*enable_updates) (void *user, const char *processor, int enable);
    void (*process_midi_arg) (void *user, int deviceID, int status, int data1, int data2);
    void (*get_midi_inputs) (void *user, void *list, DbmAddDevice add);
    void (*get_midi_outputs) (void *user, void *list, DbmAddDevice add);
    int (*open_midi_input) (void *user, int deviceID);
    int (*open_midi_output) (void *user, int deviceID);
    void (*close_midi_input) (void *user, int deviceID);
    void (*close_midi_output) (void *user, int deviceID);
    void (*send_midi_data) (void *user, int deviceID, const uint8_t *data, int length);
    void (*set_note_input_filters) (void *user, int deviceID, int noteInputIndex, const char *const *filters, int count);
    void (*set_note_input_key_translation_table) (void *user, int deviceID, int noteInputIndex, const int *table, int count);
    void (*set_note_input_velocity_translation_table) (void *user, int deviceID, int noteInputIndex, const int *table, int count);
} DbmCallbacks;

/* The callback table must stay valid until dbm_shutdown. */
void dbm_startup (const DbmCallbacks *callbacks, const char *iniPath, const char *appVersion);
void dbm_add_device (const char *name, const char *identifier);
void dbm_start_infrastructure (void);
void dbm_update_model (const char *data);
void dbm_shutdown (void);
void dbm_display_window (void);
void dbm_display_project_window (void);
void dbm_display_parameter_window (void);
void dbm_restart_controllers (void);
void dbm_set_default_document_settings (void);
/* The result must be released with dbm_free_string. */
char *dbm_get_formatted_document_settings (void);
void dbm_free_string (char *text);
void dbm_set_formatted_document_settings (const char *data);
void dbm_on_midi_event (int deviceID, const uint8_t *data, int length);

#ifdef __cplusplus
}
#endif

#endif /* DRIVENBYMOSS_H */
