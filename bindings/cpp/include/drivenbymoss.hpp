// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// Header-only C++ wrapper (C++14) around the C interface in drivenbymoss.h (rust/include).
// Add both rust/include and rust/bindings/cpp/include to the include path.
//
// Usage:
//   class MyBackend : public dbm::Backend { ... override what the host supports ... };
//   MyBackend backend;
//   dbm::App app (backend, iniPath, appVersion, DBM_FLAG_HOST_TICK);
//   app.startInfrastructure ();
//   ... app.updateModel (data); app.onMidiEvent (id, bytes, size); app.tick (); ...
//
// The backend must outlive the app. All methods of the backend have empty default
// implementations; the open functions report failure and the device lists are empty by default.
// The rules of drivenbymoss.h apply, especially: arguments are only valid during the call and
// methods of the same app must not be called from within a backend method.

#ifndef DRIVENBYMOSS_HPP
#define DRIVENBYMOSS_HPP

#include <cstdint>
#include <string>
#include <utility>
#include <vector>

#include "drivenbymoss.h"


namespace dbm
{

/** A MIDI device of the host: its ID and name. */
struct MidiDevice
{
    int id;
    std::string name;
};


/**
 * The functions the host provides to the library (the C callbacks of DbmCallbacks).
 */
class Backend
{
public:
    virtual ~Backend () = default;

    /** Execute a command without a value, e.g. ("transport", "play"). */
    virtual void processNoArg (const std::string &processor, const std::string &command) { (void) processor; (void) command; }
    virtual void processStringArg (const std::string &processor, const std::string &command, const std::string &value) { (void) processor; (void) command; (void) value; }
    virtual void processStringArgs (const std::string &processor, const std::string &command, const std::vector<std::string> &values) { (void) processor; (void) command; (void) values; }
    virtual void processIntArg (const std::string &processor, const std::string &command, int value) { (void) processor; (void) command; (void) value; }
    virtual void processDoubleArg (const std::string &processor, const std::string &command, double value) { (void) processor; (void) command; (void) value; }
    /** Delay the updates of the processor, to prevent that old values are sent back. */
    virtual void delayUpdates (const std::string &processor) { (void) processor; }
    virtual void enableUpdates (const std::string &processor, bool enable) { (void) processor; (void) enable; }
    /** Send a MIDI message to the DAW as if it was received on the MIDI input. */
    virtual void processMidiArg (int deviceId, int status, int data1, int data2) { (void) deviceId; (void) status; (void) data1; (void) data2; }
    virtual std::vector<MidiDevice> getMidiInputs () { return {}; }
    virtual std::vector<MidiDevice> getMidiOutputs () { return {}; }
    virtual bool openMidiInput (int deviceId) { (void) deviceId; return false; }
    virtual bool openMidiOutput (int deviceId) { (void) deviceId; return false; }
    virtual void closeMidiInput (int deviceId) { (void) deviceId; }
    virtual void closeMidiOutput (int deviceId) { (void) deviceId; }
    /** Send a MIDI message (short message or sysex) to a MIDI output. */
    virtual void sendMidiData (int deviceId, const uint8_t *data, int length) { (void) deviceId; (void) data; (void) length; }
    /** Filters (hex strings like "90", "B040") of the events which go directly to the DAW. */
    virtual void setNoteInputFilters (int deviceId, int noteInputIndex, const std::vector<std::string> &filters) { (void) deviceId; (void) noteInputIndex; (void) filters; }
    /** Translation table (128 entries, -1 blocks a note) for the notes of a note input. */
    virtual void setNoteInputKeyTranslationTable (int deviceId, int noteInputIndex, const std::vector<int> &table) { (void) deviceId; (void) noteInputIndex; (void) table; }
    virtual void setNoteInputVelocityTranslationTable (int deviceId, int noteInputIndex, const std::vector<int> &table) { (void) deviceId; (void) noteInputIndex; (void) table; }
};


namespace detail
{
    inline std::string str (const char *text) { return text == nullptr ? std::string () : std::string (text); }

    inline std::vector<std::string> strings (const char *const *values, int count)
    {
        std::vector<std::string> result;
        if (values != nullptr)
            for (int i = 0; i < count; i++)
                result.push_back (str (values[i]));
        return result;
    }

    inline std::vector<int> ints (const int *values, int count)
    {
        if (values == nullptr || count <= 0)
            return {};
        return std::vector<int> (values, values + count);
    }

    inline Backend &backend (void *user) { return *static_cast<Backend *> (user); }

    // The trampolines must not let exceptions escape into the library
    #define DBM_GUARD(code) try { code; } catch (...) {}

    inline void noArg (void *u, const char *p, const char *c) { DBM_GUARD (backend (u).processNoArg (str (p), str (c))) }
    inline void stringArg (void *u, const char *p, const char *c, const char *v) { DBM_GUARD (backend (u).processStringArg (str (p), str (c), str (v))) }
    inline void stringArgs (void *u, const char *p, const char *c, const char *const *v, int n) { DBM_GUARD (backend (u).processStringArgs (str (p), str (c), strings (v, n))) }
    inline void intArg (void *u, const char *p, const char *c, int v) { DBM_GUARD (backend (u).processIntArg (str (p), str (c), v)) }
    inline void doubleArg (void *u, const char *p, const char *c, double v) { DBM_GUARD (backend (u).processDoubleArg (str (p), str (c), v)) }
    inline void delayUpdates (void *u, const char *p) { DBM_GUARD (backend (u).delayUpdates (str (p))) }
    inline void enableUpdates (void *u, const char *p, int e) { DBM_GUARD (backend (u).enableUpdates (str (p), e != 0)) }
    inline void midiArg (void *u, int d, int s, int d1, int d2) { DBM_GUARD (backend (u).processMidiArg (d, s, d1, d2)) }

    inline void addDevices (const std::vector<MidiDevice> &devices, void *list, DbmAddDevice add)
    {
        for (const MidiDevice &device : devices)
            add (list, device.id, device.name.c_str ());
    }

    inline void midiInputs (void *u, void *list, DbmAddDevice add) { DBM_GUARD (addDevices (backend (u).getMidiInputs (), list, add)) }
    inline void midiOutputs (void *u, void *list, DbmAddDevice add) { DBM_GUARD (addDevices (backend (u).getMidiOutputs (), list, add)) }
    inline int openInput (void *u, int d) { try { return backend (u).openMidiInput (d) ? 1 : 0; } catch (...) { return 0; } }
    inline int openOutput (void *u, int d) { try { return backend (u).openMidiOutput (d) ? 1 : 0; } catch (...) { return 0; } }
    inline void closeInput (void *u, int d) { DBM_GUARD (backend (u).closeMidiInput (d)) }
    inline void closeOutput (void *u, int d) { DBM_GUARD (backend (u).closeMidiOutput (d)) }
    inline void sendMidi (void *u, int d, const uint8_t *data, int length) { DBM_GUARD (backend (u).sendMidiData (d, data, length)) }
    inline void filters (void *u, int d, int i, const char *const *f, int n) { DBM_GUARD (backend (u).setNoteInputFilters (d, i, strings (f, n))) }
    inline void keyTable (void *u, int d, int i, const int *t, int n) { DBM_GUARD (backend (u).setNoteInputKeyTranslationTable (d, i, ints (t, n))) }
    inline void velocityTable (void *u, int d, int i, const int *t, int n) { DBM_GUARD (backend (u).setNoteInputVelocityTranslationTable (d, i, ints (t, n))) }

    #undef DBM_GUARD

    inline DbmCallbacks callbacks (Backend &backend)
    {
        DbmCallbacks c;
        c.user = &backend;
        c.process_no_arg = noArg;
        c.process_string_arg = stringArg;
        c.process_string_args = stringArgs;
        c.process_int_arg = intArg;
        c.process_double_arg = doubleArg;
        c.delay_updates = delayUpdates;
        c.enable_updates = enableUpdates;
        c.process_midi_arg = midiArg;
        c.get_midi_inputs = midiInputs;
        c.get_midi_outputs = midiOutputs;
        c.open_midi_input = openInput;
        c.open_midi_output = openOutput;
        c.close_midi_input = closeInput;
        c.close_midi_output = closeOutput;
        c.send_midi_data = sendMidi;
        c.set_note_input_filters = filters;
        c.set_note_input_key_translation_table = keyTable;
        c.set_note_input_velocity_translation_table = velocityTable;
        return c;
    }
}


/** The version of the library. */
inline std::string version ()
{
    return detail::str (dbm_version ());
}


/**
 * An instance of DrivenByMoss (RAII wrapper of DbmApp). Not copyable, movable.
 */
class App
{
public:
    /**
     * Create an instance.
     *
     * @param backend The functions of the host, must outlive the app
     * @param iniPath The folder of the configuration files
     * @param appVersion The version of the host application, e.g. "7.22/linux-x86_64"
     * @param flags A combination of the DBM_FLAG_* values
     */
    App (Backend &backend, const std::string &iniPath, const std::string &appVersion, uint32_t flags = 0)
    {
        const DbmCallbacks callbacks = detail::callbacks (backend);
        this->app = dbm_create (&callbacks, iniPath.c_str (), appVersion.c_str (), flags);
    }

    ~App () { dbm_destroy (this->app); }

    App (const App &) = delete;
    App &operator= (const App &) = delete;

    App (App &&other) noexcept : app (other.app) { other.app = nullptr; }

    App &operator= (App &&other) noexcept
    {
        std::swap (this->app, other.app);
        return *this;
    }

    /** True if the instance was created successfully. */
    bool isValid () const noexcept { return this->app != nullptr; }

    /** Access to the C handle. */
    DbmApp *handle () const noexcept { return this->app; }

    void addDevice (const std::string &name, const std::string &identifier) { dbm_add_device (this->app, name.c_str (), identifier.c_str ()); }
    void startInfrastructure () { dbm_start_infrastructure (this->app); }
    void tick () { dbm_tick (this->app); }
    void updateModel (const std::string &data) { dbm_update_model (this->app, data.c_str ()); }
    void onMidiEvent (int deviceId, const uint8_t *data, int length) { dbm_on_midi_event (this->app, deviceId, data, length); }
    void displayWindow () { dbm_display_window (this->app); }
    void displayProjectWindow () { dbm_display_project_window (this->app); }
    void displayParameterWindow () { dbm_display_parameter_window (this->app); }
    void restartControllers () { dbm_restart_controllers (this->app); }
    void setDefaultDocumentSettings () { dbm_set_default_document_settings (this->app); }
    void setFormattedDocumentSettings (const std::string &data) { dbm_set_formatted_document_settings (this->app, data.c_str ()); }

    std::string getFormattedDocumentSettings ()
    {
        char *text = dbm_get_formatted_document_settings (this->app);
        std::string result = detail::str (text);
        dbm_free_string (text);
        return result;
    }

private:
    DbmApp *app{ nullptr };
};

} // namespace dbm

#endif /* DRIVENBYMOSS_HPP */
