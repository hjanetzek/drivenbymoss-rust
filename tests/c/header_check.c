/* Compile check of the generated header (C and C++), used by tests/ffi.rs. */
#include <string.h>

#include "drivenbymoss.h"

static void no_arg (void *user, const char *processor, const char *command) { (void) user; (void) processor; (void) command; }

int main (void)
{
    DbmCallbacks callbacks;
    memset (&callbacks, 0, sizeof (callbacks));
    callbacks.process_no_arg = no_arg;
    DbmApp *app = dbm_create (&callbacks, "", "7.0", DBM_FLAG_HOST_TICK);
    dbm_tick (app);
    char *settings = dbm_get_formatted_document_settings (app);
    dbm_free_string (settings);
    dbm_destroy (app);
    return dbm_version () == 0;
}
