-- Written by Jürgen Moßgraber - mossgrabers.de
-- (c) 2017-2026
-- Lua port: Copyright (c) 2026 Hannes Janetzek
-- Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

-- The Novation Launch Control XL in Lua: a port of the Rust version
-- (src/controller/launchcontrol/, which follows the Java LaunchControlXL* classes) on the
-- framework bindings (doc/LUA_API.md). It behaves like the Rust version and uses the same MIDI
-- discovery, so a configuration picks one or the other.
--
-- The device has 16 templates: 8 user templates (MIDI channels 1-8, their MIDI is forwarded to
-- the DAW) and 8 factory templates (MIDI channels 9-16). The factory templates 1-5 select the
-- main modes (mixer, equalizer, layer mixer, drum sequencer, note sequencer). All controls are
-- re-bound to the MIDI channel of the selected template.
--
-- Differences to Rust (and Java):
-- - The state of the former LaunchControlXLControlSurface / XlState is the table `state` of this
--   script (one Lua state per instance).
-- - The Java class hierarchy of the main modes (XLAbstractMainMode and its sub-classes) is one
--   table "class" (MainMode) with a `kind`, as XlMainMode in Rust. The built-in Parameter mode is
--   its base (the former AbstractParameterMode).
-- - Instead of the downcasts of the Rust version (`as_any().downcast_ref::<XlMainMode>()`), the
--   script keeps its main mode tables by ID (`main_modes`).
-- - Indices are 0-based, as in Rust and Java.

-- ------------------------------------------------------------------------------------------
-- Colors (former LaunchControlXLColorManager)

local COLOR_BLACK = 12
local COLOR_RED = 15
local COLOR_RED_LO = 13
local COLOR_YELLOW = 62
local COLOR_YELLOW_LO = 29
local COLOR_GREEN = 60
local COLOR_GREEN_LO = 28
local COLOR_AMBER = 63
local COLOR_AMBER_LO = 30
local COLOR_ON = 127

-- ------------------------------------------------------------------------------------------
-- The MIDI controls (former LaunchControlXLControlSurface)

-- Knobs & Faders - MIDI CC
local KNOB_SEND_A_1 = 0x0D
local KNOB_SEND_B_1 = 0x1D
local KNOB_PAN_1 = 0x31
local FADER_1 = 0x4D

-- Buttons - MIDI CC
local SEND_PREV = 0x68
local SEND_NEXT = 0x69
local TRACK_PREV = 0x6A
local TRACK_NEXT = 0x6B

-- Buttons - MIDI Notes
local DEVICE = 0x69
local MUTE = 0x6A
local SOLO = 0x6B
local RECORD_ARM = 0x6C
local TRACK_FOCUS_1 = 0x29
local TRACK_FOCUS_5 = 0x39
local TRACK_CONTROL_1 = 0x49
local TRACK_CONTROL_5 = 0x59

-- The header of the template change system exclusive message (F0h 00h 20h 29h 02h 11h 77h
-- Template F7h)
local TEMPLATE_CHANGE_HEADER = { 0xF0, 0x00, 0x20, 0x29, 0x02, 0x11, 0x77 }

-- The clip which is edited by the sequencer modes: 8 steps, 127 rows
local CLIP_STEPS = 8
local CLIP_ROWS = 127
local SEQUENCER_STEPS = 8

-- The modes of the track button mode manager which are only temporarily active (former
-- XLTemporaryButtonMode)
local TEMPORARY_BUTTON_MODES = { DeviceParams = true, InstrumentDeviceParams = true, Transport = true, Clip = true, Configuration = true }

-- The MIDI filters of the user templates: route all CC, note on and note off from the user
-- templates on channel 1-8 to the DAW
local USER_TEMPLATE_FILTERS = {
    "B0????", "B1????", "B2????", "B3????", "B4????", "B5????", "B6????", "B7????", "90????", "91????", "92????", "93????", "94????", "95????", "96????",
    "97????", "80????", "81????", "82????", "83????", "84????", "85????", "86????", "87????",
}

-- ------------------------------------------------------------------------------------------
-- The controller specific state (former XlState)

local state = {
    -- The active template on the device (0-15), -1 if not yet known
    template_id = -1,
    -- True if the device parameters are controlled by the 3rd knob row, otherwise panning
    is_device_active = false,
    -- The mode manager for the 2nd button row (former getTrackButtonModeManager)
    track_modes = nil,
    -- The mode manager for the faders (former getFaderModeManager)
    fader_modes = nil,
    -- The bind type and MIDI control of the buttons (they are re-bound on a template change)
    button_controls = {},
    -- The selected drum pad of the drum sequencer (former XLDrumSequencerMode.selectedPad)
    selected_pad = 0,
    -- The sequencer resolution index (former XLDrumSequencerMode.selectedResolutionIndex)
    drum_resolution_index = 4,
    -- The edited note of the drum pad edit mode (former XLDrumPadEditMode.note)
    drum_note = 0,
    -- The 'has been used' flags of the temporary button modes
    temporary_mode_used = {},
}

-- The main mode tables by mode ID (for the knob LEDs and the parameter banks of the track mix
-- mode, the Rust version downcasts the active mode instead)
local main_modes = {}

local function reset_has_been_used(mode)
    state.temporary_mode_used[mode] = false
end

local function set_has_been_used(mode)
    state.temporary_mode_used[mode] = true
end

local function has_been_used(mode)
    return state.temporary_mode_used[mode] == true
end

-- Mark the active track button mode as used if it is a temporary one
local function set_active_track_mode_used(ctx)
    local id = ctx:active_mode_id(state.track_modes)
    if id and TEMPORARY_BUTTON_MODES[id] then
        set_has_been_used(id)
    end
end

-- Send the state of a button LED (former setTrigger). The MIDI channel is the active
-- template. Nothing is sent as long as the template is not known.
local function set_trigger(ctx, bind_type, control, value)
    local channel = state.template_id
    if channel < 0 then
        return
    end
    if bind_type == "note" then
        ctx.surface:send_note(channel, control, value)
    else
        ctx.surface:send_cc(channel, control, value)
    end
end

local function clamp(value, min, max)
    if value < min then return min end
    if value > max then return max end
    return value
end

-- Set the color of a knob LED (former setKnobLEDColor). Red and green are in the range of 0-3.
local function set_knob_led_color(ctx, row, column, green, red)
    local midi_channel = state.template_id
    if midi_channel >= 8 then
        local g = clamp(green, 0, 3)
        local r = clamp(red, 0, 3)
        -- Red is in bit 0 and 1. Green is in bit 4 and 5.
        ctx.surface:send_note(midi_channel, 13 + row + column * 16, 12 + r + (g << 4))
    end
end

-- Select a template on the device (former selectTemplate)
local function select_template(ctx, template_id)
    local data = {}
    for i, byte in ipairs(TEMPLATE_CHANGE_HEADER) do
        data[i] = byte
    end
    data[#data + 1] = template_id
    data[#data + 1] = 0xF7
    ctx.surface:send_sysex(data)
end

-- The index of a button of the 2nd row (0-7) or nil
local function row2_index(button)
    return dbm.button_index(button, "Row21", 8)
end

-- Rust f64::round: half away from zero
local function round(value)
    if value >= 0 then
        local r = math.floor(value)
        if value - r >= 0.5 then r = r + 1 end
        return math.tointeger(r)
    end
    return -round(-value)
end

-- The LED intensity (0-3) for a knob value
local function intensity(value)
    if value == 0 then
        return 0
    end
    return value // 42 + 1
end

-- Former StringUtils.formatPercentage (as in the Rust main modes)
local function format_percentage(value)
    local formatted = string.format("%.1f", value * 100.0)
    if formatted:sub(-2) == ".0" then
        formatted = formatted:sub(1, -3)
    end
    return formatted .. "%"
end

-- Former StringUtils.formatPercentage (as in the Rust note parameters): 1 fraction digit
local function format_percentage_1(value)
    local percent = round(value * 1000.0) / 10.0
    if percent == math.floor(percent) then
        return string.format("%d%%", math.tointeger(percent))
    end
    return string.format("%.1f%%", percent)
end

-- ------------------------------------------------------------------------------------------
-- The note parameters of the note edit modes (former NoteParameter, as used by
-- XLBaseNoteEditMode). The edited note is the one of the step of the knob column, see
-- note_position. Reaper supports only the pitch of the attributes used here; with the step grid
-- of the host (Bitwig) all attributes are supported.

local NOTE_ATTRIBUTE_NAMES = { Pitch = "Pitch", Chance = "Chance", Repeat = "Repeat", VelocitySpread = "Velocity Spread", Panning = "Panning" }

local function note_clip(ctx)
    return ctx.model:note_clip(CLIP_STEPS, CLIP_ROWS)
end

local function is_supported(ctx, attribute)
    if attribute == "Pitch" then
        return true
    end
    local clip = note_clip(ctx)
    return clip ~= nil and clip:is_host_grid()
end

-- The note which is edited by the parameter with the given index (former
-- XLBaseNoteEditMode.getNotePosition). In the drum mode it is the note of the selected pad, in
-- the note mode the highest note of the step.
local function note_position(ctx, is_drum, parameter_index)
    local column = parameter_index % 8
    local channel = ctx.config:midi_edit_channel()
    local note_row
    if is_drum then
        note_row = state.drum_note
    else
        local clip = note_clip(ctx)
        note_row = clip and clip:highest_row(channel, column) or -1
    end
    return { channel = channel, step = column, note = note_row }
end

local function step_of(ctx, position)
    local clip = note_clip(ctx)
    if clip == nil then
        return { state = "off", muted = false, velocity = 0, chance = 1.0, repeat_count = 0, velocity_spread = 0.0, pan = 0.0, formatted_repeat_count = "" }
    end
    return clip:step(position.channel, position.step, position.note)
end

local function has_note(ctx, position)
    local clip = note_clip(ctx)
    return clip ~= nil and clip:step(position.channel, position.step, position.note).state ~= "off"
end

local function move_note(ctx, position, new_note)
    note_clip(ctx):move_step_y(position.channel, position.step, position.note, new_note)
    ctx:notify("Note: " .. dbm.format_note(new_note, -3))
end

local NoteParameter = {}
NoteParameter.__index = NoteParameter

local function note_parameter(index, attribute, is_drum)
    return setmetatable({ index = index, attribute = attribute, is_drum = is_drum }, NoteParameter)
end

function NoteParameter:position(ctx)
    return note_position(ctx, self.is_drum, self.index)
end

-- There is always one note position
function NoteParameter:does_exist(ctx)
    return true
end

function NoteParameter:name(ctx)
    return NOTE_ATTRIBUTE_NAMES[self.attribute]
end

function NoteParameter:value(ctx)
    local position = self:position(ctx)
    if position.note == -1 or not has_note(ctx, position) or not is_supported(ctx, self.attribute) then
        return 0
    end
    local step = step_of(ctx, position)
    local normalized_value
    if self.attribute == "Pitch" then
        normalized_value = position.note / 127.0
    elseif self.attribute == "Chance" then
        normalized_value = step.chance
    elseif self.attribute == "Repeat" then
        normalized_value = (step.repeat_count + 127.0) / 254.0
    elseif self.attribute == "VelocitySpread" then
        normalized_value = step.velocity_spread
    else
        normalized_value = (step.pan + 1.0) / 2.0
    end
    return ctx.model:value_changer():from_normalized(normalized_value)
end

function NoteParameter:displayed_value(ctx)
    local position = self:position(ctx)
    if not is_supported(ctx, self.attribute) or not has_note(ctx, position) then
        return ""
    end
    local step = step_of(ctx, position)
    if self.attribute == "Pitch" then
        return dbm.format_note(position.note, -3)
    elseif self.attribute == "Chance" then
        return format_percentage_1(step.chance)
    elseif self.attribute == "Repeat" then
        return step.formatted_repeat_count
    elseif self.attribute == "VelocitySpread" then
        return format_percentage_1(step.velocity_spread)
    end
    return format_percentage_1(step.pan)
end

function NoteParameter:set_normalized_value(ctx, value)
    if not is_supported(ctx, self.attribute) then
        return
    end
    local position = self:position(ctx)
    if not has_note(ctx, position) then
        return
    end
    local clip = note_clip(ctx)
    local ch, x, y = position.channel, position.step, position.note
    local message
    if self.attribute == "Pitch" then
        -- Move (transpose) the note up and down
        local new_note = round(value * 126.0)
        local scales = ctx.model:scales()
        if not scales:is_chromatic() then
            new_note = scales:nearest_note_in_scale(new_note)
        end
        move_note(ctx, position, new_note)
        return
    elseif self.attribute == "Chance" then
        clip:update_step_chance(ch, x, y, value)
        message = "Chance: " .. format_percentage_1(clip:step(ch, x, y).chance)
    elseif self.attribute == "Repeat" then
        -- The range is limited to 1/16 to 16 to make it controllable with an absolute knob
        clip:update_step_repeat_count(ch, x, y, round((value - 0.5) * 30.0))
        message = "Repeat: " .. clip:step(ch, x, y).formatted_repeat_count
    elseif self.attribute == "VelocitySpread" then
        clip:update_step_velocity_spread(ch, x, y, value)
        message = "Velocity Spread: " .. round(value * 100.0) .. "%"
    else
        local pan = value * 2.0 - 1.0
        clip:update_step_pan(ch, x, y, pan)
        message = "Panning: " .. format_percentage_1(pan)
    end
    ctx:notify(message)
end

function NoteParameter:change_value(ctx, control)
    if not is_supported(ctx, self.attribute) then
        return
    end
    local position = self:position(ctx)
    if not has_note(ctx, position) then
        return
    end
    local ch, x, y = position.channel, position.step, position.note
    if self.attribute ~= "Pitch" then
        local clip = note_clip(ctx)
        local message
        if self.attribute == "Chance" then
            clip:change_step_chance(ch, x, y, control)
            message = "Chance: " .. format_percentage_1(clip:step(ch, x, y).chance)
        elseif self.attribute == "Repeat" then
            clip:change_step_repeat_count(ch, x, y, control)
            message = "Repeat: " .. clip:step(ch, x, y).formatted_repeat_count
        elseif self.attribute == "VelocitySpread" then
            clip:change_step_velocity_spread(ch, x, y, control)
            message = "Vel. Spread: " .. format_percentage_1(clip:step(ch, x, y).velocity_spread)
        else
            clip:change_step_pan(ch, x, y, control)
            message = "Pan: " .. format_percentage_1(clip:step(ch, x, y).pan)
        end
        ctx:notify(message)
        return
    end
    -- Move (transpose) the note up and down
    local offset = ctx.model:value_changer():is_increase(control) and 1 or -1
    local scales = ctx.model:scales()
    local is_chromatic = scales:is_chromatic()
    local new_note = position.note
    while true do
        new_note = new_note + offset
        -- No more scale notes found?
        if new_note < 0 or new_note > 127 then
            return
        end
        if is_chromatic or scales:is_in_scale(scales:to_note_in_octave(new_note)) then
            break
        end
    end
    move_note(ctx, position, new_note)
end

function NoteParameter:reset_value(ctx)
    if not is_supported(ctx, self.attribute) then
        return
    end
    local position = self:position(ctx)
    if not has_note(ctx, position) then
        return
    end
    local clip = note_clip(ctx)
    local ch, x, y = position.channel, position.step, position.note
    if self.attribute == "Chance" then
        return clip:update_step_chance(ch, x, y, 1.0)
    elseif self.attribute == "Repeat" then
        return clip:update_step_repeat_count(ch, x, y, 0)
    elseif self.attribute == "VelocitySpread" then
        return clip:update_step_velocity_spread(ch, x, y, 0.0)
    elseif self.attribute == "Panning" then
        return clip:update_step_pan(ch, x, y, 0.0)
    end
    move_note(ctx, position, 60 + ctx.model:scales():scale_offset())
end

-- ------------------------------------------------------------------------------------------
-- The main modes of the factory templates (former XLAbstractMainMode, XLAbstractTrackMode,
-- XLTrackMixMode, XLEqMode, XLLayerMixMode, XLBaseNoteEditMode, XLDrumPadEditMode,
-- XLNoteEditMode). The kind is one of TrackMix, Eq, LayerMix, DrumPadEdit, NoteEdit.

-- The names of the parameter banks of the track mix mode (former MODE_NAMES)
local MODE_NAMES = { "Cursor Device Parameters", "Track Parameters", "Project Parameters" }
-- The parameter banks of the track mix mode
local TRACK_MIX_BANKS = { "CursorDevice", "CursorTrack", "Project" }

-- Add an equalizer to the selected track if it has none (part of the former
-- XLEqMode.trackSelectionChanged, also called when the mode is activated)
local function add_missing_equalizer(ctx)
    local eq = ctx.model:device("Eq")
    if eq and eq:exists() then
        return
    end
    local bank = ctx.model:current_track_bank()
    local index = bank:selected_index()
    if index then
        bank:item(index):add_equalizer_device()
    end
end

local MainMode = {}
MainMode.__index = MainMode

local function new_main_mode(ctx, name, kind, bank, controls, default_mode)
    local mode = setmetatable({
        kind = kind,
        -- Only the name is taken from the base (former AbstractParameterMode), the other
        -- methods are implemented here or use the defaults of the mode trait
        forward = {},
        base = ctx:create_mode("Parameter", { name = name, bank = bank, controls = controls }),
        default_mode = default_mode,
        track_mode = nil,
        -- True between activation and deactivation: a repeated activation (to rebind the knobs
        -- when the parameters of the device change) keeps the track button mode
        is_active = false,
        was_long = false,
        transport_used = false,
        new_command = ctx:command("New"),
        tempo_down = ctx:command("Tempo", { increase = false }),
        tempo_up = ctx:command("Tempo", { increase = true }),
    }, MainMode)
    return mode
end

function MainMode:is_note_edit()
    return self.kind == "DrumPadEdit" or self.kind == "NoteEdit"
end

function MainMode:is_track()
    return self.kind == "TrackMix" or self.kind == "Eq"
end

-- Former setParameterProviders
function MainMode:set_parameter_providers(ctx, parameter_provider, parameter_provider_with_device_params)
    self.parameter_provider = parameter_provider
    self.parameter_provider_with_device_params = parameter_provider_with_device_params
    self:update_provider(ctx)
end

function MainMode:update_provider(ctx)
    local provider = state.is_device_active and self.parameter_provider_with_device_params or self.parameter_provider
    self.base:set_parameter_provider(ctx, provider)
end

-- Former toggleDeviceActive
function MainMode:toggle_device_active(ctx)
    state.is_device_active = not state.is_device_active
    self:update_provider(ctx)
    self.base:bind_controls(ctx)
end

-- Former XLTrackMixMode
local function track_mix_mode(ctx, controls)
    local send1 = ctx:provider("send", 0, 0)
    local send2 = ctx:provider("send", 1, 0)
    local pan = ctx:provider("pan")
    local device_param_providers = {}
    for i, bank in ipairs(TRACK_MIX_BANKS) do
        device_param_providers[i] = ctx:provider("bank", bank)
    end
    local mode = new_main_mode(ctx, "Send A, B & Panning", "TrackMix", "track", controls, "Mute")
    mode.selected_device_param_provider_index = 0
    mode.device_param_providers = device_param_providers
    mode.send_parameter_provider1 = send1
    mode.send_parameter_provider2 = send2
    -- Control sends and pan, or sends and device parameters
    mode:set_parameter_providers(ctx, ctx:provider("combined", { send1, send2, pan }), ctx:provider("combined", { send1, send2, device_param_providers[1] }))
    return mode
end

-- Former XLEqMode
local function eq_mode(ctx, controls)
    local eq_parameters = {}
    for band = 0, 7 do
        eq_parameters[#eq_parameters + 1] = ctx:parameter("eq_band_type", band)
    end
    for band = 0, 7 do
        eq_parameters[#eq_parameters + 1] = ctx:parameter("bank_item", "Eq", dbm.eq_parameter_index("q", band))
    end
    local frequency_parameters = {}
    for band = 0, 7 do
        frequency_parameters[#frequency_parameters + 1] = ctx:parameter("bank_item", "Eq", dbm.eq_parameter_index("frequency", band))
    end
    local eq_parameter_provider = ctx:provider("fixed", eq_parameters)
    local frequency_parameter_provider = ctx:provider("fixed", frequency_parameters)
    local device_parameter_provider = ctx:provider("bank", "CursorDevice")
    local mode = new_main_mode(ctx, "Equalizer", "Eq", "track", controls, "Mute")
    -- The band types for the LEDs
    mode.band_types = {}
    for band = 0, 7 do
        mode.band_types[band] = eq_parameters[band + 1]
    end
    -- Control EQ type, Q and frequency, or EQ type, Q and device parameters
    mode:set_parameter_providers(ctx, ctx:provider("combined", { eq_parameter_provider, frequency_parameter_provider }),
        ctx:provider("combined", { eq_parameter_provider, device_parameter_provider }))
    return mode
end

-- Former XLLayerMixMode. Reaper always has drum pads, therefore the bank is the drum pad bank of
-- the first instrument (former parametersAdjusted).
local function layer_mix_mode(ctx, controls)
    local send1 = ctx:provider("layer_send", 0)
    local send2 = ctx:provider("layer_send", 1)
    local pan = ctx:provider("layer_pan")
    local device = ctx:provider("bank", "FirstInstrument")
    local mode = new_main_mode(ctx, "Layer", "LayerMix", "drum_pads:FirstInstrument", controls, "DeviceLayerMute")
    mode:set_parameter_providers(ctx, ctx:provider("combined", { send1, send2, pan }), ctx:provider("combined", { send1, send2, device }))
    return mode
end

-- The note parameter providers of the note edit modes (former XLBaseNoteEditMode)
local function note_provider(ctx, is_drum, attribute)
    local parameters = {}
    for i = 0, 7 do
        parameters[#parameters + 1] = ctx:parameter("custom", note_parameter(i, attribute, is_drum))
    end
    return ctx:provider("fixed", parameters)
end

-- Former XLBaseNoteEditMode + XLDrumPadEditMode / XLNoteEditMode. The Rec Arm button must
-- already exist.
local function note_edit_mode(ctx, controls, is_drum)
    ctx.model:scales():set_drum_default_offset(8)
    local chance = note_provider(ctx, is_drum, "Chance")
    local repeat_provider = note_provider(ctx, is_drum, "Repeat")
    local velocity_spread = note_provider(ctx, is_drum, "VelocitySpread")
    local pan = note_provider(ctx, is_drum, "Panning")
    local device = ctx:provider("bank", "FirstInstrument")

    local first_row = is_drum and chance or note_provider(ctx, is_drum, "Pitch")
    local note_edit_provider = ctx:provider("combined", { first_row, repeat_provider, pan })
    local note_edit_with_device_params_provider = ctx:provider("combined", { first_row, repeat_provider, device })
    local shifted_parameter_provider = ctx:provider("combined", { chance, velocity_spread, pan })

    local mode
    if is_drum then
        mode = new_main_mode(ctx, "Drum Sequencer", "DrumPadEdit", "none", controls, "DrumSequencer")
    else
        mode = new_main_mode(ctx, "Note Sequencer", "NoteEdit", "none", controls, "NoteSequencer")
    end
    mode:set_parameter_providers(ctx, note_edit_provider, note_edit_with_device_params_provider)
    mode.base:set_button_parameter_provider(ctx, "RecArm", shifted_parameter_provider)

    if is_drum then
        state.drum_note = ctx.model:scales():drum_offset()
    end
    -- Force clip creation
    note_clip(ctx)
    return mode
end

-- Former handleRow0
function MainMode:handle_row0(ctx, index, event)
    if event == "down" then
        if ctx.surface:is_pressed("RecArm") then
            self.transport_used = true
            self:handle_transport(ctx, index, event)
            return
        end
        self.transport_used = false
        self.was_long = false
        ctx:set_temporary_mode_in(state.fader_modes, "Master")
    elseif event == "long" then
        self.was_long = true
    else
        if self.transport_used then
            self:handle_transport(ctx, index, event)
            return
        end
        ctx:restore_mode_in(state.fader_modes)
        if not self.was_long then
            self:execute_row0(ctx, index)
        end
    end
end

-- Former alternativeModeSelect
local function alternative_mode_select(ctx, event, mode_id, alt_mode_id)
    if event == "down" then
        ctx:set_temporary_mode_in(state.track_modes, alt_mode_id)
        return
    end
    if event == "up" then
        ctx:restore_mode_in(state.track_modes)
        if has_been_used(alt_mode_id) then
            return
        end
        if mode_id then
            ctx:set_active_mode_in(state.track_modes, mode_id)
        end
    end
end

-- Former handleTransport
function MainMode:handle_transport(ctx, index, event)
    local is_down = event == "down"
    if index == 0 then
        -- New clip
        if is_down then self.new_command:create_clip(ctx, false) end
    elseif index == 1 then
        -- Toggle Launcher Overdub
        if is_down then ctx.model:toggle_launcher_overdub() end
    elseif index == 2 then
        -- Decrease tempo
        self.tempo_down:execute(ctx, event)
    elseif index == 3 then
        -- Increase tempo
        self.tempo_up:execute(ctx, event)
    elseif index == 4 then
        -- Undo
        if is_down then ctx.model:application():undo() end
    elseif index == 5 then
        -- Redo
        if is_down then ctx.model:application():redo() end
    elseif index == 6 then
        -- Toggle Clip Automation Write
        if is_down then ctx.model:transport():toggle_write_clip_launcher_automation() end
    elseif index == 7 then
        -- Toggle plug-in window
        if is_down then ctx.model:cursor_device():toggle_window_open() end
    end
end

local function choose(on, hi, lo)
    if on then return hi end
    return lo
end

-- Former getTransportButtonColor
local function transport_button_color(ctx, index)
    if index == 0 then
        -- New clip
        return COLOR_YELLOW_LO
    elseif index == 1 then
        -- Toggle Launcher Overdub
        return choose(ctx.model:is_launcher_overdub(), COLOR_AMBER, COLOR_AMBER_LO)
    elseif index == 2 or index == 3 then
        -- Tempo
        return COLOR_GREEN_LO
    elseif index == 4 then
        -- Undo
        return choose(ctx.model:application():can_undo(), COLOR_YELLOW_LO, COLOR_BLACK)
    elseif index == 5 then
        -- Redo
        return choose(ctx.model:application():can_redo(), COLOR_YELLOW_LO, COLOR_BLACK)
    elseif index == 6 then
        -- Toggle Clip Automation Write
        return choose(ctx.model:transport():is_writing_clip_launcher_automation(), COLOR_RED, COLOR_RED_LO)
    elseif index == 7 then
        return choose(ctx.model:cursor_device():is_window_open(), COLOR_GREEN, COLOR_GREEN_LO)
    end
    return COLOR_BLACK
end

-- Former executeRow0
function MainMode:execute_row0(ctx, index)
    if self:is_track() then
        local track = ctx.model:track(index)
        if not track:exists() then
            return
        end
        if track:is_selected() and track:is_group() then
            track:toggle_group_expanded()
            return
        end
        track:select()
        ctx:notify_selected("track")
    elseif self.kind == "LayerMix" then
        if not ctx.model:layer(index):exists() then
            return
        end
        self.base:select_item(ctx, index)
        ctx:notify_selected("layer")
    elseif self.kind == "DrumPadEdit" then
        state.drum_note = ctx.model:scales():drum_offset() + index
        state.selected_pad = index
        local device = ctx.model:drum_device()
        if device then
            local pad = device:drum_pad(index)
            if pad:exists() then
                ctx.host:show_notification("Drum Pad: " .. pad:name())
            end
        end
    else
        local channel = ctx.config:midi_edit_channel()
        local note_row = note_position(ctx, false, index).note
        local scale_offset = ctx.model:scales():scale_offset()
        local clip = note_clip(ctx)
        if note_row == -1 then
            -- Use the note of the currently selected scale base
            clip:toggle_step(channel, index, 60 + scale_offset, 127)
        else
            clip:clear_step(channel, index, note_row)
        end
    end
end

-- Handle the device button with the given temporary device parameter page mode
function MainMode:handle_device_button(ctx, event, temp_mode)
    if event == "down" then
        ctx:set_temporary_mode_in(state.track_modes, temp_mode)
        return
    end
    if event == "up" then
        if not has_been_used(temp_mode) then
            self:toggle_device_active(ctx)
        end
        ctx:restore_mode_in(state.track_modes)
    end
end

-- Former handleRow2
function MainMode:handle_row2(ctx, index, event)
    if self:is_track() then
        if index == 0 then
            -- Device button
            self:handle_device_button(ctx, event, "DeviceParams")
        elseif index == 1 then
            -- Mute button
            if event == "down" then
                ctx:set_active_mode_in(state.track_modes, "Mute")
            end
        elseif index == 2 then
            -- Solo button
            alternative_mode_select(ctx, event, "Solo", "Clip")
        elseif index == 3 then
            -- Record Arm button
            alternative_mode_select(ctx, event, "RecArm", "Transport")
        end
    elseif self.kind == "LayerMix" then
        if index == 0 then
            self:handle_device_button(ctx, event, "InstrumentDeviceParams")
        elseif index == 1 then
            if event == "down" then
                ctx:set_active_mode_in(state.track_modes, "DeviceLayerMute")
            end
        elseif index == 2 then
            alternative_mode_select(ctx, event, "DeviceLayerSolo", "Clip")
        elseif index == 3 then
            alternative_mode_select(ctx, event, nil, "Transport")
        end
    else
        local active = ctx:active_mode_id_ignore_temporary(state.track_modes)
        local sequencer_mode = self.default_mode
        local function toggle(mode)
            if active == mode then return sequencer_mode end
            return mode
        end
        if index == 0 then
            self:handle_device_button(ctx, event, "InstrumentDeviceParams")
        elseif index == 1 then
            alternative_mode_select(ctx, event, toggle("DeviceLayerMute"), "Configuration")
        elseif index == 2 then
            alternative_mode_select(ctx, event, toggle("DeviceLayerSolo"), "Clip")
        elseif index == 3 then
            alternative_mode_select(ctx, event, toggle("LoopLength"), "Transport")
        end
    end
end

local function bank_value(ctx, bank, index)
    return ctx.model:parameter_bank(bank):value(index)
end

-- Former getParamBank of the track mix mode
function MainMode:track_mix_param_bank()
    if self.kind == "TrackMix" then
        return TRACK_MIX_BANKS[self.selected_device_param_provider_index + 1]
    end
    return "CursorDevice"
end

-- Former XLTrackMixMode.selectParameterMode
function MainMode:select_parameter_mode(ctx, increase)
    if self.kind ~= "TrackMix" then
        return
    end
    local index = self.selected_device_param_provider_index
    if increase then
        index = math.min(index + 1, 2)
    else
        index = math.max(index - 1, 0)
    end
    self.selected_device_param_provider_index = index
    -- Former updateDeviceParametersProvider
    local second = ctx:provider("combined", { self.send_parameter_provider1, self.send_parameter_provider2, self.device_param_providers[index + 1] })
    self:set_parameter_providers(ctx, self.parameter_provider, second)
    self.base:bind_controls(ctx)
    ctx.host:show_notification(MODE_NAMES[index + 1])
end

-- The colors of the equalizer band types: green and red. Center is Bell (green). Left side
-- chooses between different low-cuts (red) and right side different high-cuts (yellow). Fully
-- left turns the band off (black). Fully right sets it to notch (amber).
local BAND_TYPE_COLORS = {
    Bell = { 3, 0 },
    Highcut = { 3, 1 },
    Highshelf = { 2, 1 },
    Lowshelf = { 0, 3 },
    Lowcut = { 0, 2 },
    Notch = { 3, 3 },
    Off = { 0, 0 },
}

-- Former IXLMode.setKnobColor
function MainMode:set_knob_color(ctx, row, column, value)
    local green, red = 0, 0
    if row == 0 then
        -- Send A / Chance intensity in green; EQ type
        if self.kind == "Eq" then
            local eq = ctx.model:device("Eq")
            local band_type = eq and eq:eq_band_type(column) or "Off"
            local colors = BAND_TYPE_COLORS[band_type]
            green, red = colors[1], colors[2]
        else
            green = intensity(value)
        end
    elseif row == 1 then
        -- Send B / Repeat intensity in red, EQ Q in green
        if self.kind == "Eq" then
            green = intensity(value)
        else
            red = intensity(value)
        end
    elseif row == 2 then
        -- Panning (EQ frequency) in amber or Device parameters yellowish
        green = intensity(value)
        if state.is_device_active then
            red = green == 0 and 0 or 1
        else
            red = green
        end
    else
        -- Not used
        return
    end
    set_knob_led_color(ctx, row, column, green, red)
end

-- Former XLDrumPadEditMode.getDrumPadColor. The key manager of the Java version is never
-- updated (the controller has no pads), therefore the 'playing' state is not used.
local function drum_pad_color(ctx, index)
    -- Selected?
    if state.selected_pad == index then
        return "COLOR_PAD_SELECTED"
    end
    -- Exists and active?
    local device = ctx.model:drum_device()
    if device == nil then
        return "COLOR_PAD_NO_CONTENT"
    end
    local drum_pad = device:drum_pad(index)
    if not drum_pad:exists() or not drum_pad:is_activated() then
        return ctx.config:is_turn_off_empty_drum_pads() and "COLOR_PAD_OFF" or "COLOR_PAD_NO_CONTENT"
    end
    -- Muted or soloed?
    if drum_pad:is_mute() or (device:has_soloed_pads() and not drum_pad:is_solo()) then
        return "COLOR_PAD_MUTED"
    end
    return "COLOR_PAD_HAS_CONTENT"
end

-- Former getFirstRowColor of the note edit modes
function MainMode:first_row_color(ctx, index)
    if self.kind == "DrumPadEdit" then
        return ctx:color_index(drum_pad_color(ctx, index))
    end
    if note_position(ctx, false, index).note < 0 then
        return COLOR_BLACK
    end
    return COLOR_AMBER
end

-- Former XLDrumPadEditMode.adjustDrumDevice
local function adjust_drum_device(ctx)
    local scales = ctx.model:scales()
    local offset = math.max(scales:drum_offset(), 0)
    local device = ctx.model:drum_device()
    if device then
        device:scroll_drum_pads_to(offset, false)
    end
    ctx.host:show_notification(scales:drum_range_text())
end

local function update_scale_properties(ctx)
    ctx.config:set_scale(ctx.model:scales():scale_name())
end

-- The send bank of a track or of a layer
local function send_bank(ctx, is_layer, index)
    if is_layer then
        return ctx.model:layer(index):send_bank()
    end
    return ctx.model:track(index):send_bank()
end

local function select_send_page(ctx, is_layer, next)
    for i = 0, 7 do
        local bank = send_bank(ctx, is_layer, i)
        if next then
            bank:select_next_page()
        else
            bank:select_previous_page()
        end
    end
    if is_layer then
        ctx:notify_selected("layer_sends", 0)
    else
        ctx:notify_selected("sends", 0)
    end
end

local function can_scroll_send_page(ctx, is_layer, next)
    for i = 0, 7 do
        local bank = send_bank(ctx, is_layer, i)
        if next then
            if bank:can_scroll_page_forwards() then return true end
        elseif bank:can_scroll_page_backwards() then
            return true
        end
    end
    return false
end

-- Select the previous/next parameter bank of the track mix mode while Device is pressed
function MainMode:select_parameter_mode_of_track_mix(ctx, increase)
    set_active_track_mode_used(ctx)
    if self.kind == "TrackMix" then
        self:select_parameter_mode(ctx, increase)
        return
    end
    local mix_mode = main_modes["Send"]
    if mix_mode then
        mix_mode:select_parameter_mode(ctx, increase)
    end
end

-- Handle the Device and Solo combinations of the page navigation. Returns true if one was
-- executed.
function MainMode:handle_page_combination(ctx, next, consume)
    local function pressed(button)
        if consume then
            return ctx:is_button_combination(button)
        end
        return ctx.surface:is_pressed(button)
    end
    if self.kind ~= "LayerMix" and pressed("Device") then
        set_active_track_mode_used(ctx)
        if next then
            ctx.model:cursor_device():select_next()
        else
            ctx.model:cursor_device():select_previous()
        end
        ctx:notify_selected("device")
        return true
    end
    if pressed("Solo") then
        set_active_track_mode_used(ctx)
        if next then
            ctx.model:scene_bank():select_next_page()
        else
            ctx.model:scene_bank():select_previous_page()
        end
        ctx:notify_selected("scene_page")
        return true
    end
    return false
end

function MainMode:select_item_page(ctx, next)
    -- As in Java, the previous page consumes the combination button, the next page not
    if self:handle_page_combination(ctx, next, not next) then
        return
    end
    if self:is_note_edit() then
        local clip = note_clip(ctx)
        if next then
            clip:scroll_steps_page_forward()
        else
            clip:scroll_steps_page_backwards()
        end
        ctx:notify_selected("edit_page", CLIP_STEPS, CLIP_ROWS)
        return
    end
    if next then
        self.base:select_next_item_page(ctx)
    else
        self.base:select_previous_item_page(ctx)
    end
    if self.kind == "LayerMix" then
        ctx:notify_selected("layer")
    else
        ctx:notify_selected("track")
    end
end

function MainMode:has_item_page(ctx, next)
    if self.kind ~= "LayerMix" and ctx.surface:is_pressed("Device") then
        local device = ctx.model:cursor_device()
        if next then return device:can_select_next() end
        return device:can_select_previous()
    end
    if ctx.surface:is_pressed("Solo") then
        local scene_bank = ctx.model:scene_bank()
        if next then return scene_bank:can_scroll_page_forwards() end
        return scene_bank:can_scroll_page_backwards()
    end
    if self:is_note_edit() then
        return true
    end
    if next then
        return self.base:has_next_item_page(ctx)
    end
    return self.base:has_previous_item_page(ctx)
end

function MainMode:select_item(ctx, next)
    if self:is_track() then
        if ctx.surface:is_pressed("Device") then
            self:select_parameter_mode_of_track_mix(ctx, next)
            return
        end
        select_send_page(ctx, false, next)
    elseif self.kind == "LayerMix" then
        select_send_page(ctx, true, next)
    elseif self.kind == "DrumPadEdit" then
        local scales = ctx.model:scales()
        if next then
            scales:inc_drum_octave()
        else
            scales:dec_drum_octave()
        end
        adjust_drum_device(ctx)
    else
        local scales = ctx.model:scales()
        if next then
            scales:next_scale()
        else
            scales:prev_scale()
        end
        update_scale_properties(ctx)
        ctx:notify_selected("scale")
    end
end

function MainMode:has_item(ctx, next)
    if self:is_track() then
        return ctx.surface:is_pressed("Device") or can_scroll_send_page(ctx, false, next)
    elseif self.kind == "LayerMix" then
        return can_scroll_send_page(ctx, true, next)
    elseif self.kind == "DrumPadEdit" then
        local scales = ctx.model:scales()
        if next then return scales:can_scroll_drum_octave_up() end
        return scales:can_scroll_drum_octave_down()
    end
    local scales = ctx.model:scales()
    if next then return scales:has_next_scale() end
    return scales:has_prev_scale()
end

-- The methods of the mode trait

function MainMode:on_activate(ctx)
    -- Make sure there is the correct provider if the settings has changed in another main mode
    self:update_provider(ctx)
    -- The app activates the mode again when the parameters of the device change (former
    -- rebindKnobs of the Java version for Reaper, which then also resets the track button mode;
    -- the Java version for Bitwig does not do this)
    if not self.is_active then
        self.is_active = true
        -- XLEqMode.onActivate; not on the repeated activations, the equalizer added by the first
        -- might not be reported yet
        if self.kind == "Eq" then
            add_missing_equalizer(ctx)
        end
        ctx:set_active_mode_in(state.track_modes, self.track_mode or self.default_mode)
    end
    self.base:on_activate(ctx)
end

function MainMode:on_deactivate(ctx)
    self.is_active = false
    self.track_mode = ctx:active_mode_id(state.track_modes)
    self.base:on_deactivate(ctx)
end

function MainMode:on_knob_value(ctx, index, value)
    if not self:is_note_edit() then
        return
    end
    -- Row 2 starts with index 8, row 4 with 16. This is only triggered from the faders.
    if index // 8 ~= 3 then
        return
    end
    local position = note_position(ctx, self.kind == "DrumPadEdit", index)
    if position.note < 0 then
        return
    end
    local clip = note_clip(ctx)
    if clip:step(position.channel, position.step, position.note).state == "off" then
        return
    end
    local normalized_value = ctx.model:value_changer():to_normalized(value)
    clip:update_step_velocity(position.channel, position.step, position.note, normalized_value)
    ctx:notify("Velocity: " .. format_percentage(normalized_value))
end

function MainMode:knob_value(ctx, index)
    local row = index // 8
    local column = index % 8
    local is_device_active = state.is_device_active
    if self.kind == "TrackMix" then
        local track = ctx.model:track(column)
        if row == 0 then
            return track:send_bank():value(0)
        elseif row == 1 then
            return track:send_bank():value(1)
        elseif row == 2 then
            if is_device_active then
                return bank_value(ctx, self:track_mix_param_bank(), column)
            end
            return track:pan()
        end
        return 0
    elseif self.kind == "Eq" then
        if row == 0 then
            return self.band_types[column]:value()
        elseif row == 1 then
            return bank_value(ctx, "Eq", dbm.eq_parameter_index("q", column))
        elseif row == 2 then
            if is_device_active then
                return bank_value(ctx, "CursorDevice", column)
            end
            return bank_value(ctx, "Eq", dbm.eq_parameter_index("frequency", column))
        end
        return 0
    elseif self.kind == "LayerMix" then
        local layer = ctx.model:layer(column)
        if row == 0 then
            return layer:send_bank():value(0)
        elseif row == 1 then
            return layer:send_bank():value(1)
        elseif row == 2 then
            if is_device_active then
                return bank_value(ctx, "FirstInstrument", column)
            end
            return layer:pan()
        end
        return 0
    end
    return self.base:knob_value(ctx, index)
end

function MainMode:on_button(ctx, row, index, event)
    if row == 0 then
        self:handle_row0(ctx, index, event)
    elseif row == 2 then
        self:handle_row2(ctx, index, event)
    end
end

local function on_off(on)
    return on and 127 or 0
end

function MainMode:button_color(ctx, button)
    local track_modes = state.track_modes
    if button == "Device" then
        return on_off(state.is_device_active)
    elseif button == "Mute" then
        return on_off(ctx:is_active_mode(self:is_track() and "Mute" or "DeviceLayerMute", track_modes))
    elseif button == "Solo" then
        return on_off(ctx:is_active_mode(self:is_track() and "Solo" or "DeviceLayerSolo", track_modes))
    elseif button == "RecArm" then
        if self:is_track() then
            return on_off(ctx:is_active_mode("RecArm", track_modes))
        elseif self.kind == "LayerMix" then
            return COLOR_BLACK
        end
        return on_off(ctx:is_active_mode("LoopLength", track_modes))
    end
    local index = dbm.button_index(button, "Row11", 8)
    if index == nil then
        return COLOR_BLACK
    end
    if ctx.surface:is_pressed("RecArm") then
        return transport_button_color(ctx, index)
    end
    if self:is_track() then
        local track = ctx.model:track(index)
        if track:exists() then
            return choose(track:is_selected(), COLOR_YELLOW, COLOR_YELLOW_LO)
        end
        return COLOR_BLACK
    end
    if self.kind == "LayerMix" then
        local layer = ctx.model:layer(index)
        if layer:exists() then
            return choose(layer:is_selected(), COLOR_YELLOW, COLOR_YELLOW_LO)
        end
        return COLOR_BLACK
    end
    return self:first_row_color(ctx, index)
end

function MainMode:select_previous_item(ctx) self:select_item(ctx, false) end
function MainMode:select_next_item(ctx) self:select_item(ctx, true) end
function MainMode:has_previous_item(ctx) return self:has_item(ctx, false) end
function MainMode:has_next_item(ctx) return self:has_item(ctx, true) end
function MainMode:select_previous_item_page(ctx) self:select_item_page(ctx, false) end
function MainMode:select_next_item_page(ctx) self:select_item_page(ctx, true) end
function MainMode:has_previous_item_page(ctx) return self:has_item_page(ctx, false) end
function MainMode:has_next_item_page(ctx) return self:has_item_page(ctx, true) end

-- The mode of the user templates: the knobs do nothing and their LEDs are off (former
-- XLDummyMode)
local function dummy_mode(ctx, controls)
    return {
        base = ctx:create_mode("Dummy", { controls = controls }),
        forward = { "on_activate", "on_deactivate", "knob_value" },
        -- Former setKnobColor: turns the LED off
        set_knob_color = function(self, ctx, row, column, value)
            set_knob_led_color(ctx, row, column, 0, 0)
        end,
    }
end

-- ------------------------------------------------------------------------------------------
-- The modes of the 2nd button row (former launchcontrol.mode.buttons). The 'temporary' modes
-- (former XLTemporaryButtonMode) store their 'has been used' flag in the controller state, since
-- the main modes need to access it.

local CHANNEL_STATE_COLORS = {
    Mute = { COLOR_AMBER, COLOR_AMBER_LO },
    Solo = { COLOR_GREEN, COLOR_GREEN_LO },
    RecArm = { COLOR_RED, COLOR_RED_LO },
}

-- Toggles mute, solo or rec arm of the tracks (former XLTrackMuteMode, XLTrackSoloMode,
-- XLTrackRecArmMode) or mute/solo of the layers (former XLLayerMuteMode, XLLayerSoloMode)
local function channel_state_mode(ctx, channel_state, is_layer)
    local kind
    if is_layer then
        kind = channel_state == "Solo" and "LayerSolo" or "LayerMute"
    else
        kind = "Track" .. channel_state
    end
    return {
        base = ctx:create_mode(kind),
        forward = { "on_activate", "on_deactivate", "on_button" },
        button_color = function(self, ctx, button)
            local index = row2_index(button)
            if index == nil then
                return COLOR_BLACK
            end
            local exists, is_on
            if is_layer then
                local layer = ctx.model:layer(index)
                exists = layer:exists()
                if channel_state == "Solo" then is_on = layer:is_solo() else is_on = layer:is_mute() end
            else
                local track = ctx.model:track(index)
                exists = track:exists()
                if channel_state == "Mute" then
                    is_on = track:is_mute()
                elseif channel_state == "Solo" then
                    is_on = track:is_solo()
                else
                    is_on = track:is_rec_arm()
                end
            end
            if not exists then
                return COLOR_BLACK
            end
            local colors = CHANNEL_STATE_COLORS[channel_state]
            return choose(is_on, colors[1], colors[2])
        end,
    }
end

-- Select a parameter page of a device (former XLSelectDeviceParamsPageMode). `id` is the ID with
-- which the mode is registered, `bank` the parameter bank of the device.
local function select_device_params_page_mode(id, bank)
    return {
        name = "Select Device Page",
        on_activate = function(self, ctx)
            reset_has_been_used(id)
        end,
        on_button = function(self, ctx, row, index, event)
            if event ~= "down" or row ~= 0 then
                return
            end
            set_has_been_used(id)
            local parameter_bank = ctx.model:parameter_bank(bank)
            if index < parameter_bank:page_count() then
                parameter_bank:select_page(index)
                ctx:notify_selected("device_and_parameter_page")
            end
        end,
        button_color = function(self, ctx, button)
            local index = row2_index(button)
            if index == nil then
                return COLOR_BLACK
            end
            local parameter_bank = ctx.model:parameter_bank(bank)
            if not parameter_bank:exists() or index >= parameter_bank:page_count() then
                return COLOR_BLACK
            end
            return choose(index == parameter_bank:selected_page(), COLOR_YELLOW, COLOR_YELLOW_LO)
        end,
    }
end

-- Transport functions (former XLTransportMode)
local function transport_mode(ctx)
    return {
        name = "Transport",
        rwd_command = ctx:command("Wind", { forward = false }),
        fwd_command = ctx:command("Wind", { forward = true }),
        play_command = ctx:command("Play"),
        on_activate = function(self, ctx)
            reset_has_been_used("Transport")
        end,
        on_button = function(self, ctx, row, index, event)
            if row ~= 0 then
                return
            end
            set_has_been_used("Transport")
            local is_down = event == "down"
            local transport = ctx.model:transport()
            if index == 0 then
                self.play_command:execute_normal(ctx, event)
            elseif index == 1 then
                if is_down then transport:start_recording() end
            elseif index == 2 then
                self.rwd_command:execute_normal(ctx, event)
            elseif index == 3 then
                self.fwd_command:execute_normal(ctx, event)
            elseif index == 4 then
                if is_down then transport:toggle_loop() end
            elseif index == 5 then
                if is_down then transport:toggle_metronome() end
            elseif index == 6 then
                if is_down then transport:toggle_write_arranger_automation() end
            elseif index == 7 then
                if is_down then
                    ctx.model:track_bank():clear_mute()
                    ctx.model:track_bank():clear_solo()
                end
            end
        end,
        button_color = function(self, ctx, button)
            local index = row2_index(button)
            if index == nil then
                return COLOR_BLACK
            end
            local transport = ctx.model:transport()
            if index == 0 then
                return choose(transport:is_playing(), COLOR_GREEN, COLOR_GREEN_LO)
            elseif index == 1 then
                return choose(transport:is_recording(), COLOR_RED, COLOR_RED_LO)
            elseif index == 2 or index == 3 then
                return COLOR_AMBER_LO
            elseif index == 4 then
                return choose(transport:is_loop(), COLOR_YELLOW, COLOR_YELLOW_LO)
            elseif index == 5 then
                return choose(transport:is_metronome_on(), COLOR_GREEN, COLOR_GREEN_LO)
            elseif index == 6 then
                return choose(transport:is_writing_arranger_automation(), COLOR_RED, COLOR_RED_LO)
            elseif index == 7 then
                return COLOR_AMBER_LO
            end
            return COLOR_BLACK
        end,
    }
end

-- Select and launch scenes (former XLScenesMode)
local scenes_mode = {
    name = "Select Scene",
    on_activate = function(self, ctx)
        reset_has_been_used("Clip")
    end,
    on_button = function(self, ctx, row, index, event)
        if row ~= 0 or event == "long" then
            return
        end
        local scene = ctx.model:scene(index)
        if not scene:exists() then
            return
        end
        local is_down = event == "down"
        if is_down then
            set_has_been_used("Clip")
            scene:select()
        end
        scene:launch(is_down, false)
    end,
    button_color = function(self, ctx, button)
        local index = row2_index(button)
        if index == nil then
            return COLOR_BLACK
        end
        local scene = ctx.model:scene(index)
        if not scene:exists() then
            return COLOR_BLACK
        end
        return choose(scene:is_selected(), COLOR_YELLOW, COLOR_YELLOW_LO)
    end,
}

-- Select the step resolution of the sequencer (former XLSequencerResolutionMode)
local sequencer_resolution_mode = {
    name = "Step Resolution",
    on_activate = function(self, ctx)
        reset_has_been_used("Configuration")
    end,
    on_button = function(self, ctx, row, index, event)
        if row ~= 0 or event ~= "down" then
            return
        end
        local value, name = dbm.resolution(index)
        note_clip(ctx):set_step_length(value)
        state.drum_resolution_index = index
        ctx.host:show_notification("Step Resolution: " .. name)
    end,
    button_color = function(self, ctx, button)
        local index = row2_index(button)
        if index == nil then
            return COLOR_BLACK
        end
        return choose(index == state.drum_resolution_index, COLOR_GREEN, COLOR_GREEN_LO)
    end,
}

-- Former isInXRange
local function is_in_x_range(clip, x)
    local step_size = clip:num_steps()
    local start = clip:edit_page() * step_size
    return x >= start and x < start + step_size
end

-- Former XLDrumSequencerMode.getStepColor (without DAW colors)
local function step_color(step_info, hilite, step)
    if step_info.state == "start" then
        -- Note starts
        if hilite then return "COLOR_STEP_HILITE_CONTENT" end
        if step_info.muted then return "COLOR_STEP_MUTED" end
        return "COLOR_CONTENT"
    elseif step_info.state == "continue" then
        -- Note continues
        if hilite then return "COLOR_STEP_HILITE_CONTENT" end
        if step_info.muted then return "COLOR_STEP_MUTED_CONT" end
        return "COLOR_CONTENT_CONT"
    end
    -- Empty
    if hilite then return "COLOR_STEP_HILITE_NO_CONTENT" end
    if step // 4 % 2 == 1 then return "COLOR_NO_CONTENT_4" end
    return "COLOR_NO_CONTENT"
end

local function highlighted_step(clip)
    local step = clip:current_step()
    if is_in_x_range(clip, step) then
        return step % SEQUENCER_STEPS
    end
    return -1
end

-- Toggle the steps of the selected drum pad (former XLDrumSequencerMode)
local function drum_sequencer_mode(ctx)
    local function note_row(ctx)
        return ctx.model:scales():drum_offset() + state.selected_pad
    end
    return {
        base = ctx:create_mode("Sequencer", { name = "Note Sequencer", clip = { CLIP_STEPS, CLIP_ROWS } }),
        forward = {},
        on_activate = function(self, ctx)
            self.base:set_selected_resolution_index(state.drum_resolution_index)
            self.base:on_activate(ctx)
        end,
        on_button = function(self, ctx, row, index, event)
            if event ~= "up" then
                return
            end
            note_clip(ctx):toggle_step(ctx.config:midi_edit_channel(), index, note_row(ctx), 127)
        end,
        -- Former getSequencerStepColor
        button_color = function(self, ctx, button)
            local index = row2_index(button)
            if index == nil then
                return COLOR_BLACK
            end
            local clip = note_clip(ctx)
            if clip == nil then
                return COLOR_BLACK
            end
            local hilite = index == highlighted_step(clip)
            local step_info = clip:step(ctx.config:midi_edit_channel(), index, note_row(ctx))
            local is_active = ctx.model:can_selected_track_hold_notes() and clip:exists()
            local color_id = is_active and step_color(step_info, hilite, index) or "COLOR_NO_CONTENT"
            return ctx:color_index(color_id)
        end,
    }
end

-- Mute/unmute the highest note of the steps (former XLNoteSequencerMode)
local function note_sequencer_mode(ctx)
    return {
        base = ctx:create_mode("Sequencer", { name = "Note Sequencer", clip = { CLIP_STEPS, CLIP_ROWS } }),
        -- AbstractSequencerMode
        forward = { "on_activate" },
        on_button = function(self, ctx, row, index, event)
            if event ~= "up" then
                return
            end
            local channel = ctx.config:midi_edit_channel()
            local clip = note_clip(ctx)
            local highest_row = clip:highest_row(channel, index)
            local step = clip:step(channel, index, highest_row)
            if step.state == "start" then
                clip:update_step_mute_state(channel, index, highest_row, not step.muted)
            end
        end,
        -- Former getSequencerStepColor
        button_color = function(self, ctx, button)
            local index = row2_index(button)
            if index == nil then
                return COLOR_BLACK
            end
            local clip = note_clip(ctx)
            if clip == nil then
                return COLOR_BLACK
            end
            local hilite = index == highlighted_step(clip)
            local channel = ctx.config:midi_edit_channel()
            local step_info = clip:step(channel, index, clip:highest_row(channel, index))
            local color_id
            if step_info.state == "start" and step_info.muted then
                color_id = hilite and "COLOR_STEP_HILITE_CONTENT" or "COLOR_STEP_MUTED"
            else
                color_id = hilite and "COLOR_STEP_HILITE_NO_CONTENT" or "COLOR_NO_CONTENT"
            end
            return ctx:color_index(color_id)
        end,
    }
end

-- Set the loop length of the clip (former XLLoopLengthMode)
local loop_length_mode = {
    name = "Set loop length",
    on_button = function(self, ctx, row, index, event)
        if row ~= 0 or event ~= "down" then
            return
        end
        local resolution = dbm.resolution(state.drum_resolution_index)
        local clip = note_clip(ctx)
        local steps = clip:edit_page() * SEQUENCER_STEPS + index + 1
        clip:set_loop_length(steps * resolution)
    end,
    button_color = function(self, ctx, button)
        local index = row2_index(button)
        if index == nil then
            return COLOR_BLACK
        end
        local clip = note_clip(ctx)
        if clip == nil then
            return COLOR_BLACK
        end
        local resolution = dbm.resolution(state.drum_resolution_index)
        local number_of_active_steps = math.floor(clip:loop_length() / resolution)
        number_of_active_steps = number_of_active_steps - clip:edit_page() * SEQUENCER_STEPS
        if index >= number_of_active_steps then
            return COLOR_BLACK
        end
        return choose(index == number_of_active_steps - 1, COLOR_AMBER, COLOR_AMBER_LO)
    end,
}

-- ------------------------------------------------------------------------------------------
-- The modes of the faders (former launchcontrol.mode.faders)

-- Edit the gains of the equalizer bands (former XLEqGainMode)
local function eq_gain_mode(ctx, controls)
    local parameters = {}
    for band = 0, 7 do
        parameters[#parameters + 1] = ctx:parameter("bank_item", "Eq", dbm.eq_parameter_index("gain", band))
    end
    return {
        base = ctx:create_mode("Parameter", { name = "EQ Gain", bank = "CursorDevice", controls = controls, provider = ctx:provider("fixed", parameters) }),
        forward = { "on_activate", "on_deactivate" },
        knob_value = function(self, ctx, index)
            local eq = ctx.model:device("Eq")
            if not (eq and eq:exists()) then
                return -1
            end
            return self.base:knob_value(ctx, index)
        end,
    }
end

-- Edit the volumes of the layers (drum pads) of the first instrument (former XLLayerVolumeMode).
-- Reaper always has drum pads, therefore the bank is the drum pad bank (former
-- parametersAdjusted).
local function layer_volume_mode(ctx, controls)
    return {
        base = ctx:create_mode("Parameter", { name = "Layer Volume", bank = "drum_pads:FirstInstrument", controls = controls, provider = ctx:provider("layer_volume") }),
        forward = { "on_activate", "on_deactivate", "knob_value" },
    }
end

-- The fader mode of the drum and note sequencer templates. In Java the note edit modes are
-- registered in the main and in the fader mode manager; their faders are not bound to
-- parameters but to KnobRowModeCommands which call the main mode. Since a mode cannot be active
-- (borrowed) in two managers at the same time in Rust, the fader manager uses this mode, which
-- does nothing (it does not bind the faders).
local function note_edit_fader_mode(name)
    return { name = name }
end

-- ------------------------------------------------------------------------------------------
-- The setup (former LaunchControlXLControllerSetup)

local function sequential_controls(first, count)
    local controls = {}
    for i = 0, count - 1 do
        controls[#controls + 1] = dbm.continuous(first, i)
    end
    return controls
end

-- The faders (former FADER_IDS)
local function fader_ids()
    return sequential_controls("Fader1", 8)
end

-- The 3 rows of knobs (former KNOB_CONTROLS)
local function knob_controls()
    local controls = {}
    for _, first in ipairs({ "Send1Knob1", "Send2Knob1", "PanKnob1" }) do
        for _, id in ipairs(sequential_controls(first, 8)) do
            controls[#controls + 1] = id
        end
    end
    return controls
end

-- The bind type of a button (former getTriggerBindType)
local function trigger_bind_type(button)
    if button == "MoveTrackLeft" or button == "MoveTrackRight" or button == "MoveBankLeft" or button == "MoveBankRight" then
        return "cc"
    end
    return "note"
end

-- The color of a button which is controlled by the active main mode (former getModeColor)
local function mode_color_supplier(button)
    return function(ctx)
        return ctx:mode_button_color(0, button)
    end
end

-- The color of a button which is controlled by the active track button mode (former
-- getTrackModeColor)
local function track_mode_color_supplier(button)
    return function(ctx)
        if ctx:active_mode_id(state.track_modes) then
            return ctx:mode_button_color(state.track_modes, button)
        end
        return COLOR_BLACK
    end
end

-- Create a button with a command and a light (former createButton). The MIDI binding is done
-- when a template is selected (see bind_to_template).
local function create_button(ctx, id, label, command, midi_control, supplier)
    local bind_type = trigger_bind_type(id)
    state.button_controls[#state.button_controls + 1] = { id, bind_type, midi_control }
    ctx.surface:button(id, {
        label = label,
        command = command,
        light = supplier,
        sender = function(ctx, value)
            set_trigger(ctx, bind_type, midi_control, value)
        end,
    })
end

-- Register the modes (former createModes)
local function create_modes(ctx)
    -- The Rec Arm button is needed by the note edit modes (shifted parameter provider)
    create_button(ctx, "RecArm", "REC ARM", ctx:command("ButtonRowMode", { row = 2, index = 3 }), RECORD_ARM, mode_color_supplier("RecArm"))

    local knobs = knob_controls()
    local faders = fader_ids()

    main_modes["Send"] = track_mix_mode(ctx, knobs)
    main_modes["EqDeviceParams"] = eq_mode(ctx, knobs)
    main_modes["DeviceLayer"] = layer_mix_mode(ctx, knobs)
    main_modes["DrumSequencer"] = note_edit_mode(ctx, knobs, true)
    main_modes["NoteSequencer"] = note_edit_mode(ctx, knobs, false)
    main_modes["Dummy"] = dummy_mode(ctx, knobs)
    for _, id in ipairs({ "Send", "EqDeviceParams", "DeviceLayer", "DrumSequencer", "NoteSequencer", "Dummy" }) do
        ctx:register_mode(id, main_modes[id])
    end

    local track_modes = state.track_modes
    ctx:register_mode_in(track_modes, "Mute", channel_state_mode(ctx, "Mute", false))
    ctx:register_mode_in(track_modes, "Solo", channel_state_mode(ctx, "Solo", false))
    ctx:register_mode_in(track_modes, "RecArm", channel_state_mode(ctx, "RecArm", false))
    ctx:register_mode_in(track_modes, "DeviceParams", select_device_params_page_mode("DeviceParams", "CursorDevice"))
    ctx:register_mode_in(track_modes, "InstrumentDeviceParams", select_device_params_page_mode("InstrumentDeviceParams", "FirstInstrument"))
    ctx:register_mode_in(track_modes, "Transport", transport_mode(ctx))
    ctx:register_mode_in(track_modes, "Clip", scenes_mode)
    ctx:register_mode_in(track_modes, "DeviceLayerMute", channel_state_mode(ctx, "Mute", true))
    ctx:register_mode_in(track_modes, "DeviceLayerSolo", channel_state_mode(ctx, "Solo", true))
    ctx:register_mode_in(track_modes, "DrumSequencer", drum_sequencer_mode(ctx))
    ctx:register_mode_in(track_modes, "NoteSequencer", note_sequencer_mode(ctx))
    ctx:register_mode_in(track_modes, "LoopLength", loop_length_mode)
    ctx:register_mode_in(track_modes, "Configuration", sequencer_resolution_mode)

    local fader_modes = state.fader_modes
    ctx:register_mode_in(fader_modes, "Volume", ctx:create_mode("TrackVolume", { controls = faders }))
    ctx:register_mode_in(fader_modes, "Master", ctx:create_mode("MasterAndFXVolume", { controls = faders }))
    ctx:register_mode_in(fader_modes, "EqDeviceParams", eq_gain_mode(ctx, faders))
    ctx:register_mode_in(fader_modes, "DeviceLayer", layer_volume_mode(ctx, faders))
    ctx:register_mode_in(fader_modes, "DrumSequencer", note_edit_fader_mode("Drum Sequencer"))
    ctx:register_mode_in(fader_modes, "NoteSequencer", note_edit_fader_mode("Note Sequencer"))
    ctx:register_mode_in(fader_modes, "Dummy", ctx:create_mode("Dummy", { controls = faders }))
end

-- Register the commands of all buttons (former registerTriggerCommands)
local function register_trigger_commands(ctx)
    local cursors = {
        { "MoveBankLeft", "Send Previous", "left", SEND_PREV },
        { "MoveBankRight", "Send Next", "right", SEND_NEXT },
        { "MoveTrackLeft", "Previous", "down", TRACK_PREV },
        { "MoveTrackRight", "Next", "up", TRACK_NEXT },
    }
    for _, cursor in ipairs(cursors) do
        local command = ctx:command("ModeCursor", { direction = cursor[3], notify = false })
        create_button(ctx, cursor[1], cursor[2], command, cursor[4], function(ctx)
            return command:can_scroll() and 127 or 0
        end)
    end

    create_button(ctx, "Device", "DEVICE", ctx:command("ButtonRowMode", { row = 2, index = 0 }), DEVICE, mode_color_supplier("Device"))
    create_button(ctx, "Mute", "MUTE", ctx:command("ButtonRowMode", { row = 2, index = 1 }), MUTE, mode_color_supplier("Mute"))
    create_button(ctx, "Solo", "SOLO", ctx:command("ButtonRowMode", { row = 2, index = 2 }), SOLO, mode_color_supplier("Solo"))

    for i = 0, 7 do
        local focus, control
        if i < 4 then
            focus, control = TRACK_FOCUS_1 + i, TRACK_CONTROL_1 + i
        else
            focus, control = TRACK_FOCUS_5 + i - 4, TRACK_CONTROL_5 + i - 4
        end
        local row1 = dbm.button("Row11", i)
        create_button(ctx, row1, "Row 1: " .. (i + 1), ctx:command("ButtonRowMode", { row = 0, index = i }), focus, mode_color_supplier(row1))
        local row2 = dbm.button("Row21", i)
        create_button(ctx, row2, "Row 2: " .. (i + 1), ctx:command("ButtonRowMode", { manager = state.track_modes, row = 0, index = i }), control,
            track_mode_color_supplier(row2))
    end
end

-- The knob rows: first control, label, index offset of the knob row mode command
local KNOB_ROWS = { { "Send1Knob1", "Send A Knob", 0 }, { "Send2Knob1", "Send B Knob", 8 }, { "PanKnob1", "Pan Knob", 16 } }

-- Get the value of one of the knobs from the active main mode (former getKnobValue)
local function knob_value(ctx, index)
    return math.max(ctx:mode_knob_value(0, index), 0)
end

-- Set the color of a knob LED by the active main mode (former setKnobRowColor)
local function set_knob_row_color(ctx, row, column, value)
    local id = ctx:active_mode_id()
    local mode = id and main_modes[id]
    if mode and mode.set_knob_color then
        mode:set_knob_color(ctx, row, column, value)
    end
end

-- Register the knobs and faders (former registerContinuousCommands). The command bindings are
-- only for 'non-parameter' modes like note editing. The MIDI binding happens separately since
-- each template has its own MIDI channel (see bind_to_template).
local function register_continuous_commands(ctx)
    for i = 0, 7 do
        for row, knob_row in ipairs(KNOB_ROWS) do
            local first, label, offset = knob_row[1], knob_row[2], knob_row[3]
            local knob_index = offset + i
            ctx.surface:continuous(dbm.continuous(first, i), { label = label .. " " .. (i + 1), kind = "absolute_knob", command = ctx:command("KnobRowMode", { index = knob_index }) })
            ctx.surface:light({
                supplier = function(ctx) return knob_value(ctx, knob_index) end,
                sender = function(ctx, value) set_knob_row_color(ctx, row - 1, i, value) end,
            })
        end
        ctx.surface:continuous(dbm.continuous("Fader1", i), { label = "Fader " .. (i + 1), kind = "fader", command = ctx:command("KnobRowMode", { index = 24 + i }) })
    end
end

-- Re-bind all controls to the MIDI channel of the selected factory template. Each template has
-- its own MIDI channel (former bindToTemplate).
local function bind_to_template(ctx)
    local midi_channel = state.template_id
    if midi_channel < 0 then
        return
    end
    local surface = ctx.surface
    for i = 0, 7 do
        surface:bind_continuous(dbm.continuous("Send1Knob1", i), "cc", midi_channel, KNOB_SEND_A_1 + i)
        surface:bind_continuous(dbm.continuous("Send2Knob1", i), "cc", midi_channel, KNOB_SEND_B_1 + i)
        surface:bind_continuous(dbm.continuous("PanKnob1", i), "cc", midi_channel, KNOB_PAN_1 + i)
        surface:bind_continuous(dbm.continuous("Fader1", i), "cc", midi_channel, FADER_1 + i)
    end
    for _, control in ipairs(state.button_controls) do
        surface:bind_button(control[1], control[2], midi_channel, control[3])
    end
    surface:clear_cache()
end

-- The main mode, the fader mode and the notification of the factory templates
local TEMPLATES = {
    -- Factory mode 1 - Track Mixer
    [8] = { "Send", "Volume", "Mixer" },
    -- Factory mode 2 - Equalizer
    [9] = { "EqDeviceParams", "EqDeviceParams", "Equalizer" },
    -- Factory mode 3 - Layer Mixer
    [10] = { "DeviceLayer", "DeviceLayer", "Layer Mixer" },
    -- Factory mode 4 - Drum Sequencer
    [11] = { "DrumSequencer", "DrumSequencer", "Drum Sequencer" },
    -- Factory mode 5 - Note Sequencer
    [12] = { "NoteSequencer", "NoteSequencer", "Note Sequencer" },
}

-- A different factory template has been selected: update all modes and bindings (former
-- activateTemplate)
local function activate_template(ctx)
    local template_id = state.template_id
    ctx.host:println("Switch to template: " .. template_id)

    local template = TEMPLATES[template_id] or { "Dummy", "Dummy", "" }
    ctx:set_active_mode(template[1])
    ctx:set_active_mode_in(state.fader_modes, template[2])

    if template_id >= 8 then
        bind_to_template(ctx)
        if template[3] ~= "" then
            ctx.host:show_notification("Selected Mode: " .. template[3])
        end
    end
end

-- Former XLEqMode.trackSelectionChanged: add an equalizer to the selected track if there is none
-- (only while the equalizer mode is active)
local function eq_track_selection_changed(ctx, is_selected)
    if not is_selected or not ctx:is_active_mode("EqDeviceParams") then
        return
    end
    add_missing_equalizer(ctx)
end

-- React on a changed setting (former createObservers: deactivated items handler and scale
-- observers)
local function handle_setting_change(ctx, id)
    if id == "EXCLUDE_DEACTIVATED_ITEMS" then
        ctx.model:track_bank():set_skip_disabled_items(ctx.config:are_deactivated_items_excluded())
    elseif id == "SCALES_SCALE" then
        ctx.model:scales():set_scale_by_name(ctx.config:scale())
    elseif id == "SCALES_BASE" then
        ctx.model:scales():set_scale_offset_by_name(ctx.config:scale_base())
    elseif id == "SCALES_IN_KEY" then
        ctx.model:scales():set_chromatic(not ctx.config:is_scale_in_key())
    end
end

-- The port names for the automatic detection (former createWindowsDeviceDiscoveryPairs etc.)
local LAUNCH_CONTROL_XL = "Launch Control XL"

local function discovery(os)
    local function pair(name)
        return { inputs = { name }, outputs = { name } }
    end
    if os == "windows" then
        -- Former createWindowsDeviceDiscoveryPairs ("%sLaunch Control XL", ...)
        local results = { pair(LAUNCH_CONTROL_XL) }
        for i = 1, 19 do
            results[#results + 1] = pair(i .. "- " .. LAUNCH_CONTROL_XL)
        end
        return results
    elseif os == "mac" then
        return { pair(LAUNCH_CONTROL_XL) }
    end
    return { pair(LAUNCH_CONTROL_XL), pair(LAUNCH_CONTROL_XL .. " Launch Contro") }
end

return {
    info = {
        vendor = "Novation",
        model = "Launch Control XL (Lua)",
        -- Not the UUID of the Rust/Java version, a configuration selects one or the other
        uuid = "4E926F54-1B67-4363-B971-D248C52F175D",
        class_name = "de.mossgrabers.reaper.controller.novation.launchcontrol.LaunchControlXLLuaControllerInstance",
        midi_inputs = 1,
        midi_outputs = 1,
    },
    discovery = discovery,
    -- Former LaunchControlXLConfiguration.init
    settings = {
        -- Scale
        { "scale", "document" },
        { "scale_base", "document" },
        { "scale_in_key", "document" },
        -- Transport
        { "behaviour_on_pause", "global" },
        { "double_click_play", "global" },
        -- Play & Sequence
        { "midi_edit_channel", "global" },
        -- Workflow
        { "exclude_deactivated_items", "global" },
        { "new_clip_length", "global" },
    },
    value_changer = { "twos_complement", 128, 1 },
    scales = { 36, 100, 8, 8 },
    colors = {
        indices = {
            BUTTON_COLOR_OFF = COLOR_BLACK,
            BUTTON_COLOR_ON = COLOR_ON,
            BUTTON_STATE_OFF = 0,
            BUTTON_STATE_ON = 1,
            BUTTON_STATE_HI = 127,
            COLOR_STEP_HILITE_NO_CONTENT = COLOR_GREEN_LO,
            COLOR_STEP_HILITE_CONTENT = COLOR_GREEN,
            COLOR_STEP_MUTED = COLOR_RED,
            COLOR_STEP_MUTED_CONT = COLOR_RED_LO,
            COLOR_STEP_SELECTED = COLOR_YELLOW,
            COLOR_NO_CONTENT = COLOR_BLACK,
            COLOR_NO_CONTENT_4 = COLOR_BLACK,
            COLOR_CONTENT = COLOR_AMBER,
            COLOR_CONTENT_CONT = COLOR_AMBER_LO,
            COLOR_PAD_OFF = COLOR_BLACK,
            COLOR_PAD_RECORD = COLOR_RED,
            COLOR_PAD_PLAY = COLOR_GREEN,
            COLOR_PAD_SELECTED = COLOR_YELLOW,
            COLOR_PAD_MUTED = COLOR_AMBER_LO,
            COLOR_PAD_HAS_CONTENT = COLOR_YELLOW_LO,
            COLOR_PAD_NO_CONTENT = COLOR_BLACK,
        },
        fill = { 0, 0, 0 },
        rgb = {
            [COLOR_BLACK] = { 0, 0, 0 },
            [COLOR_RED] = "ReddishBrown",
            [COLOR_RED_LO] = { 39, 4, 1 },
            [COLOR_AMBER] = "ReddishBrown",
            [COLOR_AMBER_LO] = "DarkBrown",
            [COLOR_YELLOW] = { 107, 105, 1 },
            [COLOR_YELLOW_LO] = { 37, 36, 1 },
            [COLOR_GREEN] = { 1, 104, 1 },
            [COLOR_GREEN_LO] = { 1, 36, 1 },
            [COLOR_ON] = { 255, 255, 255 },
        },
    },
    -- Former createModel
    model = function(config)
        return {
            flat_track_list = true,
            full_flat_track_list = config.master_tracks_included,
            num_sends = 2,
            num_drum_pad_layers = 8,
            devices = { "Eq", "FirstInstrument" },
            track_bank_indication = true,
        }
    end,
    midi_input_filters = USER_TEMPLATE_FILTERS,

    -- Former init: createModes, registerTriggerCommands, registerContinuousCommands
    create = function(ctx)
        state.track_modes = ctx:add_mode_manager()
        state.fader_modes = ctx:add_mode_manager()
        create_modes(ctx)
        register_trigger_commands(ctx)
        register_continuous_commands(ctx)
    end,

    startup = function(ctx)
        ctx:set_active_mode_in(state.track_modes, "Mute")
        ctx:set_active_mode_in(state.fader_modes, "Volume")
        select_template(ctx, 8)
    end,

    -- Former createObservers
    handle_changes = function(ctx)
        for _, event in ipairs(ctx.model:take_events()) do
            if event.type == "TrackSelectionChanged" then
                eq_track_selection_changed(ctx, event.is_selected)
            end
        end
        for _, id in ipairs(ctx.config:take_changed_settings()) do
            handle_setting_change(ctx, id)
        end
    end,

    exit = function(ctx)
        ctx:turn_off_lights()
    end,

    -- Handle the system exclusive messages of the device (former handleSysEx)
    handle_sysex = function(ctx, data)
        local bytes = dbm.from_hex(data)
        -- Handle template change - F0h 00h 20h 29h 02h 11h 77h Template F7h
        if #bytes == 9 then
            local matches = true
            for i, byte in ipairs(TEMPLATE_CHANGE_HEADER) do
                if bytes[i] ~= byte then
                    matches = false
                    break
                end
            end
            if matches then
                local template_id = bytes[8]
                -- Already active?
                if state.template_id == template_id then
                    return
                end
                state.template_id = template_id
                activate_template(ctx)
                return
            end
        end
        ctx.host:error("Unhandled Sysex: " .. data)
    end,

    -- Ignore 'not bound' warnings for user modes (former handleCC)
    handle_midi = function(ctx, status, data1, data2) end,

    -- For tests and debugging (scripts::eval)
    state = state,
}
