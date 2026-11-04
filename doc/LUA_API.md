# The Lua controller API

Generated from the bindings (`src/script/lua/bindings.rs`) by `cargo test --lib -- --ignored write_lua_api` (the test `lua_api_is_up_to_date` checks it). See doc/LUA_PLAN.md for the design and `controllers/launch_control_xl.lua` for an example.

## Conventions

- A script returns a table: `info` (`vendor`, `model`, `uuid`, `class_name`, `midi_inputs`, `midi_outputs`), `discovery` (a function of the OS name `windows`/`linux`/`mac` or a table by OS, a list of `{inputs = {…}, outputs = {…}}`), `settings` (names of generic settings or `{name, "global"|"document"}`), `value_changer` (`{"twos_complement", upper_bound, step}`), `colors` (`{indices = {[color id] = index}, rgb = {[index] = {r, g, b} | DAW color name}, fill = {r, g, b}}`), `scales` (`{start_note, end_note, cols, rows}`), `model` (a table or a function of `{master_tracks_included = bool}`: `flat_track_list`, `full_flat_track_list`, `num_tracks`, `num_scenes`, `num_sends`, `num_devices_in_bank`, `num_params`, `num_drum_pad_layers`, `devices = {"Eq", "FirstInstrument"}`, `track_bank_indication`), `midi_input_filters`, and the functions `create(ctx)`, `startup(ctx)`, `handle_changes(ctx)`, `flush(ctx)` (default `ctx:flush_surface()`), `exit(ctx)` (default `ctx:turn_off_lights()`), `handle_sysex(ctx, hex)`, `handle_midi(ctx, status, data1, data2)`.
- Indices are 0-based, as in the Rust and Java versions.
- IDs are the names of the Rust enum variants: buttons (`"Row11"`), continuous controls (`"Fader1"`), modes (`"Send"`), views; parameter banks `CursorDevice`, `FirstInstrument`, `Eq`, `DrumDevice`, `CursorTrack`, `Project`; button events `"down"`, `"up"`, `"long"`; bind types `"cc"`, `"note"`, `"pitchbend"`, `"program_change"`.
- `ctx` is read-only in lights, colors and values (functions which change something raise an error there).
- A mode table has any of the functions of the `Mode` trait (`on_activate(self, ctx)`, `on_button(self, ctx, row, index, event)`, `button_color(self, ctx, button)`, `knob_value(self, ctx, index)`, …), an optional `name`, an optional `base` (a built-in mode) and an optional `forward` list: the methods without a function go to the base (default: all).
- An error in a script function is logged with the script and the line; the call has no effect, and a function which fails three times in a row is disabled.

## dbm

Global functions (the table `dbm`).

- `dbm.button(first, offset) -> string`: The button `offset` positions after `first` (former `ButtonID.get`), e.g. `dbm.button("Row11", 2)` is `"Row13"`.
- `dbm.continuous(first, offset) -> string`: The continuous control `offset` positions after `first`, e.g. `dbm.continuous("Fader1", 7)`.
- `dbm.button_index(button, first, count) -> integer|nil`: The offset of `button` from `first` if it is one of the `count` buttons from there.
- `dbm.resolution(index) -> number, string`: The value (in beats) and the name of a sequencer resolution (0-7, former `Resolution`).
- `dbm.format_note(note, octave_offset) -> string`: The name and octave of a MIDI note (former `Scales.formatNoteAndOctave`).
- `dbm.eq_parameter_index(kind, band) -> integer`: The index of a parameter of an equalizer band in the parameter bank `Eq`: kind `"gain"`, `"q"` or `"frequency"`.
- `dbm.from_hex(text) -> {integer}`: The bytes of a hex string (as `handle_sysex` gets it).

## Ctx

The context of a call (`ctx`). Valid only during the call it is passed to.

- `.model -> Model`: The DAW model.
- `.surface -> Surface`: The control surface.
- `.host -> Host`: The host.
- `.config -> Config`: The configuration.
- `:is_valid() -> bool`: True during the call which got this ctx.
- `:add_mode_manager() -> integer`: Add a mode manager (0 is the main one).
- `:create_mode(kind, options) -> Mode`: Create a built-in mode: `Parameter`, `Sequencer`, `Dummy`, `MasterAndFXVolume`, `Track`, `TrackSelect`, `TrackMute`, `TrackSolo`, `TrackRecArm`, `TrackStopClip`, `TrackVolume`, `TrackPan`, `TrackSend`, `LayerMute`, `LayerSolo`. Options: `name`, `controls` (list of IDs), `absolute` (default true), `bank` (`none`, `track`, a parameter bank, `drum_pads:FirstInstrument|Drum|Cursor`), `provider`, `clip = {steps, rows}`, `send`.
- `:register_mode_in(manager, id, mode)`: Register a mode (a table or a built-in mode) in a mode manager.
- `:register_mode(id, mode)`: Register a mode in the main mode manager.
- `:set_active_mode_in(manager, id)`: Activate a mode.
- `:set_active_mode(id)`: Activate a mode of the main mode manager.
- `:set_temporary_mode_in(manager, id)`: Activate a mode temporarily (see `restore_mode_in`).
- `:restore_mode_in(manager)`: End the temporary mode (or go back to the previous one).
- `:active_mode_id(manager = 0) -> string|nil`: The ID of the active mode (the temporary one if set).
- `:active_mode_id_ignore_temporary(manager = 0) -> string|nil`: The ID of the active mode, not the temporary one.
- `:is_active_mode(id, manager = 0) -> bool`: True if the mode is active (or temporary).
- `:mode_button_color(manager, button) -> integer`: The color of a button from the active mode of the manager (0 without an active mode).
- `:mode_knob_value(manager, index) -> integer`: The value of a knob from the active mode of the manager (0 without an active mode).
- `:create_view(kind) -> View`: Create a built-in view: `ControlOnly`.
- `:register_view(id, view)`: Register a view (a table or a built-in view).
- `:set_active_view(id)`: Activate a view.
- `:active_view_id() -> string|nil`: The ID of the active view.
- `:command(kind, options) -> Command`: Create a built-in command: `ButtonRowMode` (`row`, `index`, `manager`), `KnobRowMode` (`index`, `manager`), `ModeCursor`/`Cursor` (`direction` left/right/up/down, `notify`), `ModeSelect` (`mode`, `manager`, `toggle`, `notify`, `temporary`), `New`, `Tempo` (`increase`), `Wind` (`forward`), `Play`.
- `:provider(kind, ...) -> Provider`: Create a parameter provider: `volume`, `pan`, `send` (index, offset = 0) of the tracks; `bank` (name) of a parameter bank; `layer_volume`, `layer_pan`, `layer_send` (index) of the first instrument; `combined` ({providers}); `fixed` ({parameters}); `empty` (size).
- `:parameter(kind, ...) -> Parameter`: Create a parameter: `empty`; `bank_item` (bank, index); `bank_position` (bank, position); `track_volume`, `track_pan` (index); `track_send` (index, send); `selected_track_volume`, `selected_track_pan`; `selected_track_send` (send); `master_volume`, `master_pan`, `metronome_volume`, `crossfader`; `eq_band_type` (band); `custom` (a table with `does_exist`, `value`, `set_normalized_value`, `change_value` and optionally `name`, `displayed_value`, `set_value`, `reset_value`, `touch_value`, each `function (self, ctx, …)`).
- `:color_index(color_id) -> integer`: The color index of a registered color ID (an error if it is not registered).
- `:notify(text)`: Show a notification (on the display or by the host).
- `:schedule(delay_ms, function (ctx))`: Run a function later (also to switch the mode or view from inside it).
- `:is_button_combination(button) -> bool`: True if the button is pressed; then it is marked as used (its release does not execute its command).
- `:flush_surface()`: Update the surface: the active view, then the lights (the default `flush`).
- `:turn_off_lights()`: Turn off all lights (the default `exit`).
- `:notify_selected(what, ...)`: The notifications of the selection (former `MVHelper`): `track`, `layer`, `device`, `parameter_page`, `device_and_parameter_page`, `sends` (track index), `layer_sends` (layer index), `scene_page`, `edit_page` (steps, rows), `scale`.

## Model

The DAW model (`ctx.model`).

- `:track_bank() -> TrackBank`: The track bank (former `getTrackBank`).
- `:current_track_bank() -> TrackBank`: The current track bank (instrument/audio or effect tracks).
- `:track(index) -> Track`: A track of the track bank.
- `:layer(index) -> Layer`: A layer (or drum pad) of the first instrument.
- `:scene_bank() -> SceneBank`: The scene bank.
- `:scene(index) -> Scene`: A scene of the scene bank.
- `:transport() -> Transport`: The transport.
- `:application() -> Application`: The application.
- `:scales() -> Scales`: The scales.
- `:value_changer() -> ValueChanger`: The value changer.
- `:cursor_device() -> Device`: The cursor device.
- `:device(name) -> Device`: A specific device: `FirstInstrument`, `Eq`, `NiKomplete`.
- `:drum_device() -> Device`: The (main) drum device.
- `:parameter_bank(name) -> ParameterBank`: A parameter bank.
- `:note_clip(steps, rows) -> Clip|nil`: The cursor clip of the size; created if it does not exist (nil in a read-only call if it does not exist).
- `:is_launcher_overdub() -> bool`: True if launcher overdub is on.
- `:toggle_launcher_overdub()`: Toggle launcher overdub.
- `:can_selected_track_hold_notes() -> bool`: True if the selected track can hold notes.
- `:take_events() -> {event}`: The events of the model since the last call (former observers): tables with `type` (`TrackSelectionChanged` with `index`, `is_selected`; `TrackBankPageChanged`; `SceneBankPageChanged`; `SendBankPageChanged` with `track_position`; `TrackNote` with `track_index`, `note`, `velocity`; `ParameterValueChanged` with `bank`, `page`, `index`; `ParameterPageChanged` with `bank`; `CurrentTrackBankChanged`).

## TrackBank

A track bank (`ctx.model:track_bank()`, `:current_track_bank()`).

- `:item(index) -> Track`: A track of the current page.
- `:selected_index() -> integer|nil`: The index of the selected track in the page.
- `:set_skip_disabled_items(skip)`: Exclude the deactivated tracks.
- `:set_indication(enable)`: Show the bank in the DAW.
- `:clear_mute()`: Unmute all tracks.
- `:clear_solo()`: Unsolo all tracks.

## Track

A track of the current page of a track bank.

- `:exists() -> bool`: True if the track exists.
- `:is_selected() -> bool`: True if the track is selected.
- `:select()`: Select the track.
- `:name() -> string`: The name.
- `:position() -> integer`: The absolute position (-1 if it does not exist).
- `:is_group() -> bool`: True for a group track.
- `:toggle_group_expanded()`: Expand or collapse the group.
- `:is_mute() -> bool`: True if muted.
- `:is_solo() -> bool`: True if soloed.
- `:is_rec_arm() -> bool`: True if armed for recording.
- `:can_hold_notes() -> bool`: True for an instrument track.
- `:volume() -> integer`: The volume (in the range of the value changer).
- `:pan() -> integer`: The panorama (in the range of the value changer).
- `:add_equalizer_device()`: Add an equalizer device to the track.
- `:send_bank() -> SendBank`: The sends of the track.

## Layer

A layer (or drum pad) of the current page of the first instrument.

- `:exists() -> bool`: True if the layer exists.
- `:is_selected() -> bool`: True if the layer is selected.
- `:name() -> string`: The name.
- `:is_mute() -> bool`: True if muted.
- `:is_solo() -> bool`: True if soloed.
- `:volume() -> integer`: The volume.
- `:pan() -> integer`: The panorama.
- `:send_bank() -> SendBank`: The sends of the layer.

## SendBank

The sends of a track or layer.

- `:value(index) -> integer`: The value of the send at the index of the page.
- `:exists(index) -> bool`: True if the send at the index of the page exists.
- `:select_next_page()`: Scroll to the next page.
- `:select_previous_page()`: Scroll to the previous page.
- `:can_scroll_page_forwards() -> bool`: True if there is a next page.
- `:can_scroll_page_backwards() -> bool`: True if there is a previous page.

## SceneBank

The scene bank (`ctx.model:scene_bank()`).

- `:item(index) -> Scene`: A scene of the current page.
- `:select_next_page()`: Scroll to the next page.
- `:select_previous_page()`: Scroll to the previous page.
- `:can_scroll_page_forwards() -> bool`: True if there is a next page.
- `:can_scroll_page_backwards() -> bool`: True if there is a previous page.

## Scene

A scene of the current page.

- `:exists() -> bool`: True if the scene exists.
- `:is_selected() -> bool`: True if the scene is selected.
- `:name() -> string`: The name.
- `:select()`: Select the scene.
- `:launch(is_pressed, is_alternative)`: Launch the scene (on press and release).

## Device

A device: the cursor device, a specific device or the drum device.

- `:exists() -> bool`: True if the device exists.
- `:name() -> string`: The name.
- `:select_next()`: Select the next device.
- `:select_previous()`: Select the previous device.
- `:can_select_next() -> bool`: True if there is a next device.
- `:can_select_previous() -> bool`: True if there is a previous device.
- `:is_window_open() -> bool`: True if the plug-in window is open.
- `:toggle_window_open()`: Open or close the plug-in window.
- `:eq_band_type(band) -> string`: The type of an equalizer band: `Off`, `Lowcut`, `Lowshelf`, `Bell`, `Highcut`, `Highshelf`, `Notch`.
- `:drum_pad(index) -> DrumPad`: A drum pad of the current page.
- `:has_soloed_pads() -> bool`: True if a drum pad is soloed.
- `:scroll_drum_pads_to(position, adjust_page)`: Scroll the drum pad bank to the position.

## DrumPad

A drum pad of the current page of a device.

- `:exists() -> bool`: True if the pad exists.
- `:is_activated() -> bool`: True if the pad is activated.
- `:is_mute() -> bool`: True if muted.
- `:is_solo() -> bool`: True if soloed.
- `:name() -> string`: The name.

## ParameterBank

A parameter bank (`ctx.model:parameter_bank(name)`).

- `:exists() -> bool`: True if the bank exists in the model.
- `:value(index) -> integer`: The value of the parameter at the index of the page (0 if there is none).
- `:page_count() -> integer`: The number of parameter pages.
- `:selected_page() -> integer`: The index of the selected page.
- `:select_page(index)`: Select a parameter page.

## Clip

A cursor clip of the given size (`ctx.model:note_clip(steps, rows)`). A step is `(channel, step, note)`.

- `:exists() -> bool`: True if the clip exists in the DAW.
- `:is_host_grid() -> bool`: True if the step grid comes from the host (all note attributes).
- `:num_steps() -> integer`: The number of steps of a page.
- `:edit_page() -> integer`: The edited page.
- `:current_step() -> integer`: The playing step (-1 if not playing).
- `:loop_length() -> number`: The loop length in beats.
- `:set_loop_length(beats)`: Set the loop length.
- `:set_step_length(beats)`: Set the length of a step.
- `:step(channel, step, note) -> table`: The step: `state` (`off`, `start`, `continue`), `muted`, `velocity`, `chance`, `repeat_count`, `velocity_spread`, `pan`, `formatted_repeat_count`.
- `:highest_row(channel, step) -> integer`: The highest note of the step (-1 if none).
- `:toggle_step(channel, step, note, velocity)`: Toggle the note of a step.
- `:clear_step(channel, step, note)`: Remove the note of a step.
- `:move_step_y(channel, step, note, new_note)`: Move the note of a step to another note.
- `:scroll_steps_page_forward()`: Edit the next page.
- `:scroll_steps_page_backwards()`: Edit the previous page.
- `:update_step_velocity(channel, step, note, value)`: Set the velocity (0..1).
- `:update_step_mute_state(channel, step, note, muted)`: Mute or unmute the note.
- `:update_step_chance(channel, step, note, value)`: Set the chance (0..1).
- `:update_step_repeat_count(channel, step, note, count)`: Set the repeat count.
- `:update_step_velocity_spread(channel, step, note, value)`: Set the velocity spread (0..1).
- `:update_step_pan(channel, step, note, value)`: Set the panorama (-1..1).
- `:change_step_chance(channel, step, note, control)`: Change the chance with a relative value.
- `:change_step_repeat_count(channel, step, note, control)`: Change the repeat count with a relative value.
- `:change_step_velocity_spread(channel, step, note, control)`: Change the velocity spread with a relative value.
- `:change_step_pan(channel, step, note, control)`: Change the panorama with a relative value.

## Transport

The transport (`ctx.model:transport()`).

- `:is_playing() -> bool`: True while playing.
- `:is_recording() -> bool`: True while recording.
- `:is_loop() -> bool`: True if the loop is on.
- `:is_metronome_on() -> bool`: True if the metronome is on.
- `:is_writing_arranger_automation() -> bool`: True if arranger automation is written.
- `:is_writing_clip_launcher_automation() -> bool`: True if clip launcher automation is written.
- `:start_recording()`: Start recording.
- `:toggle_loop()`: Toggle the loop.
- `:toggle_metronome()`: Toggle the metronome.
- `:toggle_write_arranger_automation()`: Toggle writing arranger automation.
- `:toggle_write_clip_launcher_automation()`: Toggle writing clip launcher automation.

## Application

The application (`ctx.model:application()`).

- `:undo()`: Undo.
- `:redo()`: Redo.
- `:can_undo() -> bool`: True if there is something to undo.
- `:can_redo() -> bool`: True if there is something to redo.

## Scales

The scales (`ctx.model:scales()`).

- `:drum_offset() -> integer`: The first note of the drum pads.
- `:set_drum_default_offset(offset)`: Set the default drum offset.
- `:inc_drum_octave()`: One drum octave up.
- `:dec_drum_octave()`: One drum octave down.
- `:can_scroll_drum_octave_up() -> bool`: True if the drum octave can go up.
- `:can_scroll_drum_octave_down() -> bool`: True if the drum octave can go down.
- `:drum_range_text() -> string`: The range of the drum pads as text.
- `:next_scale()`: Select the next scale.
- `:prev_scale()`: Select the previous scale.
- `:has_next_scale() -> bool`: True if there is a next scale.
- `:has_prev_scale() -> bool`: True if there is a previous scale.
- `:scale_name() -> string`: The name of the scale.
- `:scale_offset() -> integer`: The offset of the scale base (0-11).
- `:is_chromatic() -> bool`: True if chromatic.
- `:set_chromatic(chromatic)`: Set chromatic.
- `:set_scale_by_name(name)`: Select a scale by name.
- `:set_scale_offset_by_name(name)`: Select the scale base by name.
- `:is_in_scale(note_in_octave) -> bool`: True if the note (0-11) is in the scale.
- `:to_note_in_octave(note) -> integer`: The note in the octave (0-11) relative to the scale base.
- `:nearest_note_in_scale(note) -> integer`: The nearest note of the scale.

## ValueChanger

The value changer of the model (`ctx.model:value_changer()`).

- `:to_normalized(value) -> number`: A value in the range of the value changer as 0..1.
- `:from_normalized(value) -> integer`: A value 0..1 in the range of the value changer.
- `:is_increase(control) -> bool`: True if the relative control value increases.
- `:upper_bound() -> integer`: The upper bound of the values.

## Surface

The control surface (`ctx.surface`).

- `:button(id, {label, command, light, sender, midi})`: Create a button: `command` is a function (ctx, event, velocity) or a built-in command; `light` a function (ctx) -> state; `sender` a function (ctx, state) which sends it; `midi = {type, channel, control}` binds it (then the default sender sends the state as the value of the control, and without `light` the state is 1 while pressed).
- `:light({supplier, sender, button})`: Create a light: `supplier` (ctx) -> state, `sender` (ctx, state); optionally the button it belongs to.
- `:continuous(id, {label, kind, command, midi})`: Create a knob or fader: `kind` is `fader`, `absolute_knob` or `relative_knob`; `command` a function (ctx, value) or a built-in command; `midi = {type, channel, control}` binds it.
- `:bind_button(id, type, channel, control)`: Bind a button to a MIDI message.
- `:bind_continuous(id, type, channel, control)`: Bind a knob or fader to a MIDI message.
- `:clear_cache()`: Forget the sent light states (all are sent again).
- `:is_pressed(id) -> bool`: True if the button is pressed.
- `:is_shift_pressed() -> bool`: True if Shift is pressed.
- `:send_sysex(bytes)`: Send a system exclusive message (a list of bytes or a hex string).
- `:send_note(channel, note, velocity)`: Send a note.
- `:send_cc(channel, cc, value)`: Send a control change.

## Host

The host (`ctx.host`).

- `:println(text)`: Log a message.
- `:error(text)`: Log an error.
- `:show_notification(text)`: Show a notification in the DAW.

## Config

The configuration (`ctx.config`).

- `:midi_edit_channel() -> integer`: The MIDI channel for editing notes (0-15).
- `:is_turn_off_empty_drum_pads() -> bool`: Turn off empty drum pads.
- `:are_deactivated_items_excluded() -> bool`: Exclude the deactivated items.
- `:are_master_tracks_included() -> bool`: Include the master tracks.
- `:scale() -> string`: The scale setting.
- `:scale_base() -> string`: The scale base setting.
- `:is_scale_in_key() -> bool`: The in key setting.
- `:set_scale(name)`: Store the scale setting.
- `:take_changed_settings() -> {string}`: The settings which changed since the last call, by the names of their constants (`SCALES_SCALE`, …; controller-specific settings as numbers).

## Mode

A built-in mode (`ctx:create_mode`), as the `base` of a mode table or registered directly.

- `:name() -> string`: The name.
- `:on_activate(ctx)`: Activate.
- `:on_deactivate(ctx)`: Deactivate.
- `:knob_value(ctx, index) -> integer`: The value of a knob.
- `:on_knob_value(ctx, index, value)`: A knob was moved.
- `:button_color(ctx, button) -> integer`: The color of a button.
- `:on_button(ctx, row, index, event)`: A button of a row.
- `:select_item(ctx, index)`: Select an item of the bank.
- `:select_previous_item(ctx)`: Select the previous item.
- `:select_next_item(ctx)`: Select the next item.
- `:select_previous_item_page(ctx)`: Select the previous page.
- `:select_next_item_page(ctx)`: Select the next page.
- `:has_previous_item(ctx) -> bool`: True if there is a previous item.
- `:has_next_item(ctx) -> bool`: True if there is a next item.
- `:has_previous_item_page(ctx) -> bool`: True if there is a previous page.
- `:has_next_item_page(ctx) -> bool`: True if there is a next page.
- `:set_parameter_provider(ctx, provider)`: `Parameter`: set the provider of the controls (former `setParameterProvider`).
- `:set_button_parameter_provider(ctx, button, provider)`: `Parameter`: the provider while the button is pressed.
- `:bind_controls(ctx)`: `Parameter`: bind the controls to the parameters of the provider.
- `:selected_resolution_index() -> integer`: `Sequencer`: the index of the step resolution.
- `:set_selected_resolution_index(index)`: `Sequencer`: set the index of the step resolution.

## View

A built-in view (`ctx:create_view`).

- `:name() -> string`: The name.

## Command

A built-in command (`ctx:command`).

- `:execute(ctx, event, velocity)`: Execute (with Shift the shifted function).
- `:execute_normal(ctx, event)`: Execute the normal function.
- `:execute_value(ctx, value)`: Execute a continuous command.
- `:can_scroll() -> bool`: `ModeCursor`/`Cursor`: true if it can scroll (for its light).
- `:create_clip(ctx, enable_overdub)`: `New`: create a new clip on the cursor track (former `NewCommand.handleExecute`).

## Provider

A parameter provider (`ctx:provider`).

- `:size() -> integer`: The number of parameters.

## Parameter

A parameter (`ctx:parameter`).

- `:value() -> integer`: The value.
- `:exists() -> bool`: True if the parameter exists.

242 functions and fields.
