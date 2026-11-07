//! Platform gamepad polling and publication to FrameXML/Lua.
//!
//! SDL is initialized before Bevy starts its window/event loop. The app owns a
//! normalized snapshot; the Lua bridge only receives a copy of that snapshot.

use benilla_ui::script::{GamepadLuaState, NearestMode, UiScript};
use bevy::prelude::*;

#[cfg(not(target_os = "ios"))]
use sdl3::gamepad::{Axis, Button, Gamepad};

/// The original controller's R3 target scan radius.
const LEGACY_R3_TARGET_RANGE_YARDS: f32 = 41.0;

#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct GamepadSnapshot {
    pub(crate) current: GamepadLuaState,
    pub(crate) previous: GamepadLuaState,
    pub(crate) touch_look_delta: Vec2,
    pub(crate) touch_look_active: bool,
    pub(crate) touch_ui_pointer: Option<Vec2>,
    pub(crate) touch_world_pointer: Option<Vec2>,
}

impl GamepadSnapshot {
    /// The old controller's radial deadzone: a centered stick is zero, then the remaining
    /// magnitude is remapped to the full range without changing its direction.
    pub(crate) fn move_axis(&self) -> Vec2 {
        let stick = if self.current.touch_move_active {
            Vec2::new(self.current.touch_move_x, self.current.touch_move_y)
        } else {
            Vec2::new(self.current.left_x, self.current.left_y)
        };
        apply_deadzone(stick, 0.15)
    }

    pub(crate) fn look_axis(&self) -> Vec2 {
        if self.current.zl > 0.5 {
            return Vec2::ZERO;
        }

        // right_y is stored positive-up for the virtual pointer (UI coordinates). The legacy
        // camera path consumes positive-down, as the original SDL GamepadState did. Flip only
        // the camera view here; virtual_mouse_axis() remains unchanged.
        let axis = self.virtual_mouse_axis();
        Vec2::new(axis.x, -axis.y)
    }

    pub(crate) fn virtual_mouse_axis(&self) -> Vec2 {
        apply_deadzone(Vec2::new(self.current.right_x, self.current.right_y), 0.15)
    }
}

#[derive(Clone, Copy, Debug)]
struct UiTouchGesture {
    id: u64,
    started_at: f32,
    start_raw: Vec2,
    start_window: Vec2,
    last_window: Vec2,
    left_down: bool,
    right_down: bool,
    eligible_click: bool,
}

#[derive(Clone, Copy, Debug)]
struct WorldTouchGesture {
    id: u64,
    start_raw: Vec2,
    previous_raw: Vec2,
    started_at: f32,
    dragging: bool,
    right_down: bool,
    window_position: Vec2,
}

#[derive(Default)]
struct TouchGestureState {
    move_id: Option<u64>,
    move_axis: Vec2,
    ui: Option<UiTouchGesture>,
    world: Option<WorldTouchGesture>,
    pending_world_release: Option<(bevy::input::mouse::MouseButton, Vec2)>,
    world_pointer: Option<Vec2>,
    ui_pointer: Option<Vec2>,
    clear_world_pointer_next: bool,
    clear_ui_pointer_next: bool,
}

fn touch_stick_contains(point: Vec2, centre: Vec2, radius: f32) -> bool {
    point.distance_squared(centre) <= radius * radius
}

fn touch_stick_claim_allowed(
    enabled: bool,
    unlocked: bool,
    gamepad_connected: bool,
    point: Vec2,
    centre: Vec2,
    radius: f32,
) -> bool {
    enabled && !unlocked && !gamepad_connected && touch_stick_contains(point, centre, radius)
}

fn touch_stick_axes(point: Vec2, centre: Vec2, travel: f32) -> Vec2 {
    ((point - centre) / travel.max(1.0)).clamp_length_max(1.0)
}

fn touch_look_started_drag(start: Vec2, current: Vec2) -> bool {
    start.distance_squared(current) >= 64.0
}

/// Translates touch into the same ordered input records as a mouse. Touch look remains a camera
/// gesture; only a finger that began on an interactive UI frame enters the UI pointer walk.
fn inject_touch_input(
    mut events: MessageReader<bevy::input::touch::TouchInput>,
    mut records: ResMut<crate::ui_script::InputRecords>,
    mut snapshot: ResMut<GamepadSnapshot>,
    mut state: Local<crate::ui_script::VmMemo<TouchGestureState>>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    widescreen: Option<Res<crate::video::Widescreen>>,
    time: Res<Time>,
    script: Option<NonSend<UiScript>>,
) {
    use crate::ui_script::{Record, UiScreen};
    use bevy::input::mouse::MouseButton;
    use bevy::input::touch::TouchPhase;
    use bevy::input::ButtonState;

    const WORLD_LONG_PRESS_SECS: f32 = 1.0;
    const UI_LONG_PRESS_SECS: f32 = 3.0;

    let state = state.get_for(script.as_deref());
    let Ok(window) = window.single() else {
        return;
    };
    let screen = UiScreen::of_window(window, widescreen.as_deref());
    let to_ui = |at: Vec2| crate::ui_script::window_to_ui(screen, at);
    let mut generated = Vec::new();
    snapshot.touch_look_delta = Vec2::ZERO;
    snapshot.touch_look_active = state.world.is_some_and(|gesture| gesture.dragging);

    // Release a world click on the frame after its press so the regular world picker can latch
    // the touch's press-position ray before the up edge arrives.
    let clear_world_now = state.clear_world_pointer_next;
    state.clear_world_pointer_next = false;
    if let Some((button, at)) = state.pending_world_release.take() {
        generated.push(pointer_record(
            Record::Button {
                button,
                state: ButtonState::Released,
            },
            at,
        ));
        state.clear_world_pointer_next = true;
    } else if clear_world_now && state.world.is_none() {
        state.world_pointer = None;
    }
    let clear_ui_now = state.clear_ui_pointer_next;
    state.clear_ui_pointer_next = false;
    if clear_ui_now && state.ui.is_none() {
        state.ui_pointer = None;
    }

    let lt_owns_pointer = snapshot.current.zl > 0.5;
    let stick = script.as_deref().map_or(
        (132.0_f32, 136.0_f32, 112.0_f32, 63.0_f32, false, false),
        UiScript::touch_stick_settings,
    );
    let centre = Vec2::new(stick.0, stick.1);
    if !stick.4 || stick.5 || snapshot.current.connected {
        // Settings may change while a finger still owns the stick (for example when a hardware
        // controller connects). Never let the stale touch vector override the physical pad.
        state.move_id = None;
        state.move_axis = Vec2::ZERO;
    }

    // Evaluate long presses even on frames with no TouchMoved event.
    if let Some(ui) = state.ui.as_mut() {
        if !ui.left_down
            && !ui.right_down
            && ui.eligible_click
            && time.elapsed_secs() - ui.started_at >= UI_LONG_PRESS_SECS
        {
            generated.push(pointer_record(
                Record::Button {
                    button: MouseButton::Right,
                    state: ButtonState::Pressed,
                },
                ui.last_window,
            ));
            ui.right_down = true;
        }
    }
    let mut pending_world_long_press_release = None;
    if let Some(world) = state.world.as_mut() {
        if !world.dragging
            && !world.right_down
            && time.elapsed_secs() - world.started_at >= WORLD_LONG_PRESS_SECS
        {
            generated.push(pointer_record(
                Record::Button {
                    button: MouseButton::Right,
                    state: ButtonState::Pressed,
                },
                world.window_position,
            ));
            pending_world_long_press_release = Some((MouseButton::Right, world.window_position));
            world.right_down = true;
        }
    }
    if pending_world_long_press_release.is_some() {
        state.pending_world_release = pending_world_long_press_release;
    }

    let touch_events: Vec<_> = events
        .read()
        .map(|touch| (touch.id, touch.phase, touch.position))
        .collect();
    for (id, phase, raw) in touch_events {
        let ui = to_ui(raw);
        let interactive_ui = script
            .as_deref()
            .is_some_and(|s| s.hit_test_is_interactive(ui.x, ui.y));
        let hit_ui = script
            .as_deref()
            .and_then(|s| s.hit_test(ui.x, ui.y))
            .is_some_and(|frame| script.as_deref().is_some_and(|s| !s.is_world_frame(frame)));
        let in_stick = touch_stick_claim_allowed(
            stick.4,
            stick.5,
            snapshot.current.connected,
            ui,
            centre,
            stick.2,
        );
        let claim_move = in_stick && !interactive_ui && state.move_id.is_none();
        let can_route_ui = !lt_owns_pointer && state.ui.is_none();

        match phase {
            TouchPhase::Started => {
                if claim_move {
                    state.move_id = Some(id);
                    state.move_axis = touch_stick_axes(ui, centre, stick.3);
                } else if can_route_ui && hit_ui {
                    state.ui = Some(UiTouchGesture {
                        id,
                        started_at: time.elapsed_secs(),
                        start_raw: raw,
                        start_window: raw,
                        last_window: raw,
                        left_down: false,
                        right_down: false,
                        eligible_click: true,
                    });
                    state.ui_pointer = Some(raw);
                    generated.push(pointer_record(Record::Move, raw));
                } else if !lt_owns_pointer && state.world.is_none() {
                    state.world = Some(WorldTouchGesture {
                        id,
                        start_raw: raw,
                        previous_raw: raw,
                        started_at: time.elapsed_secs(),
                        dragging: false,
                        right_down: false,
                        window_position: raw,
                    });
                    state.world_pointer = Some(raw);
                }
            }
            TouchPhase::Moved if state.move_id == Some(id) => {
                state.move_axis = touch_stick_axes(ui, centre, stick.3);
            }
            TouchPhase::Moved if state.ui.is_some_and(|gesture| gesture.id == id) => {
                let mut gesture = state.ui.expect("matched UI touch");
                let crossed = touch_look_started_drag(gesture.start_raw, raw);
                if crossed && !gesture.left_down && !gesture.right_down {
                    generated.push(pointer_record(
                        Record::Button {
                            button: MouseButton::Left,
                            state: ButtonState::Pressed,
                        },
                        gesture.start_window,
                    ));
                    gesture.left_down = true;
                }
                gesture.eligible_click &= !crossed;
                gesture.last_window = raw;
                state.ui_pointer = Some(raw);
                generated.push(pointer_record(Record::Move, raw));
                state.ui = Some(gesture);
            }
            TouchPhase::Moved if state.world.is_some_and(|gesture| gesture.id == id) => {
                let mut gesture = state.world.expect("matched world touch");
                let was_dragging = gesture.dragging;
                gesture.dragging |= touch_look_started_drag(gesture.start_raw, raw);
                if gesture.dragging {
                    snapshot.touch_look_delta += if was_dragging {
                        raw - gesture.previous_raw
                    } else {
                        raw - gesture.start_raw
                    };
                    snapshot.touch_look_active = true;
                }
                gesture.previous_raw = raw;
                gesture.window_position = raw;
                state.world_pointer = Some(raw);
                state.world = Some(gesture);
            }
            TouchPhase::Ended | TouchPhase::Canceled if state.move_id == Some(id) => {
                state.move_id = None;
                state.move_axis = Vec2::ZERO;
            }
            TouchPhase::Ended | TouchPhase::Canceled
                if state.ui.is_some_and(|gesture| gesture.id == id) =>
            {
                let gesture = state.ui.take().expect("matched UI touch");
                state.ui_pointer = Some(raw);
                generated.push(pointer_record(Record::Move, raw));
                if gesture.right_down {
                    generated.push(pointer_record(
                        Record::Button {
                            button: MouseButton::Right,
                            state: ButtonState::Released,
                        },
                        raw,
                    ));
                } else if gesture.left_down {
                    generated.push(pointer_record(
                        Record::Button {
                            button: MouseButton::Left,
                            state: ButtonState::Released,
                        },
                        raw,
                    ));
                } else if phase == TouchPhase::Ended {
                    let button = if time.elapsed_secs() - gesture.started_at >= UI_LONG_PRESS_SECS {
                        MouseButton::Right
                    } else {
                        MouseButton::Left
                    };
                    generated.push(pointer_record(
                        Record::Button {
                            button,
                            state: ButtonState::Pressed,
                        },
                        raw,
                    ));
                    generated.push(pointer_record(
                        Record::Button {
                            button,
                            state: ButtonState::Released,
                        },
                        raw,
                    ));
                }
                state.clear_ui_pointer_next = true;
            }
            TouchPhase::Ended | TouchPhase::Canceled
                if state.world.is_some_and(|gesture| gesture.id == id) =>
            {
                let gesture = state.world.take().expect("matched world touch");
                state.world_pointer = Some(raw);
                if phase == TouchPhase::Ended && !gesture.dragging && !gesture.right_down {
                    let button =
                        if time.elapsed_secs() - gesture.started_at >= WORLD_LONG_PRESS_SECS {
                            MouseButton::Right
                        } else {
                            MouseButton::Left
                        };
                    generated.push(pointer_record(
                        Record::Button {
                            button,
                            state: ButtonState::Pressed,
                        },
                        raw,
                    ));
                    state.pending_world_release = Some((button, raw));
                } else {
                    state.clear_world_pointer_next = true;
                }
            }
            _ => {}
        }
    }

    snapshot.current.touch_move_active = state.move_id.is_some();
    snapshot.current.touch_move_x = state.move_axis.x;
    snapshot.current.touch_move_y = state.move_axis.y;
    snapshot.touch_ui_pointer = state.ui_pointer;
    snapshot.touch_world_pointer = state.world_pointer;
    snapshot.touch_look_active |= state.world.is_some_and(|gesture| gesture.dragging);

    let touch_owns_pointer = state.move_id.is_some()
        || state.ui.is_some()
        || state.world.is_some()
        || state.pending_world_release.is_some()
        || state.world_pointer.is_some();
    if !generated.is_empty() || touch_owns_pointer {
        // Keyboard records survive while touch owns the pointer; mouse records cannot race it.
        records.inject_gamepad_pointer(generated, touch_owns_pointer, false);
    }
}

/// The virtual mouse owned by LT mode. Its logical position survives releasing LT; the active
/// window point is published for the UI walk and the world hover pick.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct GamepadVirtualMouse {
    pub(crate) active: bool,
    initialized: bool,
    pub(crate) position_ui: Vec2,
    pub(crate) window_position: Option<Vec2>,
    left_down: bool,
    right_down: bool,
}

fn advance_virtual_mouse(position: Vec2, stick: Vec2, delta_secs: f32, bounds: Vec2) -> Vec2 {
    let delta = Vec2::new(stick.x, -stick.y) * 700.0 * delta_secs.max(0.0);
    (position + delta).clamp(Vec2::ZERO, bounds.max(Vec2::ZERO))
}

fn pointer_record(record: crate::ui_script::Record, at: Vec2) -> crate::ui_script::InputRecord {
    crate::ui_script::InputRecord {
        record,
        mods: Default::default(),
        at: Some(at),
    }
}

/// Synthesizes pointer records after the OS records have been collected, preserving the normal UI
/// and world-click path. While LT is held, real mouse records are ignored and the right stick
/// moves the virtual cursor instead.
fn update_gamepad_virtual_mouse(
    snapshot: Res<GamepadSnapshot>,
    mut mouse: ResMut<GamepadVirtualMouse>,
    mut records: ResMut<crate::ui_script::InputRecords>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    widescreen: Option<Res<crate::video::Widescreen>>,
    time: Res<Time>,
) {
    use crate::ui_script::{Record, UiScreen};
    use bevy::input::mouse::MouseButton;
    use bevy::input::ButtonState;

    let active = snapshot.current.connected && snapshot.current.zl > 0.5;
    if !active {
        if mouse.active {
            if let Some(at) = mouse.window_position {
                let mut release = Vec::new();
                if mouse.left_down {
                    release.push(pointer_record(
                        Record::Button {
                            button: MouseButton::Left,
                            state: ButtonState::Released,
                        },
                        at,
                    ));
                }
                if mouse.right_down {
                    release.push(pointer_record(
                        Record::Button {
                            button: MouseButton::Right,
                            state: ButtonState::Released,
                        },
                        at,
                    ));
                }
                // Release the virtual buttons before this frame's real mouse events.
                records.inject_gamepad_pointer(release, false, true);
            }
            mouse.left_down = false;
            mouse.right_down = false;
        }
        mouse.active = false;
        mouse.window_position = None;
        return;
    }

    let Ok(window) = window.single() else {
        return;
    };
    let screen = UiScreen::of_window(window, widescreen.as_deref());
    let scale = crate::ui_script::seam_scale(screen.size.y);
    let bounds = screen.size / scale;
    if !mouse.initialized {
        mouse.position_ui = bounds * 0.5;
        mouse.initialized = true;
    }

    let previous_window_position = mouse.window_position;
    mouse.position_ui = advance_virtual_mouse(
        mouse.position_ui,
        snapshot.virtual_mouse_axis(),
        time.delta_secs(),
        bounds,
    );
    let at = crate::ui_script::ui_to_window(screen, mouse.position_ui);
    let entered = !mouse.active;
    let moved =
        previous_window_position.is_none_or(|previous| previous.distance_squared(at) > 0.01);

    let mut generated = Vec::new();
    if entered || moved {
        generated.push(pointer_record(Record::Move, at));
    }
    if snapshot.current.a != mouse.left_down {
        generated.push(pointer_record(
            Record::Button {
                button: MouseButton::Left,
                state: if snapshot.current.a {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                },
            },
            at,
        ));
    }
    if snapshot.current.b != mouse.right_down {
        generated.push(pointer_record(
            Record::Button {
                button: MouseButton::Right,
                state: if snapshot.current.b {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                },
            },
            at,
        ));
    }

    // Do not let an OS mouse move/click race the LT-controlled virtual pointer.
    records.inject_gamepad_pointer(generated, true, false);
    mouse.active = true;
    mouse.window_position = Some(at);
    mouse.left_down = snapshot.current.a;
    mouse.right_down = snapshot.current.b;
}

fn apply_deadzone(value: Vec2, deadzone: f32) -> Vec2 {
    let magnitude = value.length();
    if magnitude <= deadzone {
        return Vec2::ZERO;
    }
    let direction = value / magnitude;
    direction * ((magnitude - deadzone) / (1.0 - deadzone)).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GamepadButton {
    A,
    B,
    X,
    Y,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

#[derive(Resource, Default)]
pub(crate) struct GamepadBindings {
    /// The action command selected on press; modifier changes must not alter its release.
    latched: Vec<(GamepadButton, &'static str)>,
    /// The modifier has been initialized for the current Lua VM.
    modifier_initialized: bool,
}

impl GamepadBindings {
    /// Drop held command edges and re-seed the modifier when the active Lua VM is replaced.
    pub(crate) fn reset_for_vm(&mut self) {
        self.latched.clear();
        self.modifier_initialized = false;
    }
}

#[cfg(not(target_os = "ios"))]
pub(crate) struct SdlGamepad {
    // Drop open handles before the SDL context they depend on.
    gamepad: Option<Gamepad>,
    subsystem: sdl3::GamepadSubsystem,
    gamepad_index: usize,
    _sdl: sdl3::Sdl,
}

#[cfg(not(target_os = "ios"))]
pub(crate) fn init_platform_gamepad() -> Option<SdlGamepad> {
    let gamepad_index = std::env::args()
        .skip(1)
        .find_map(|arg| {
            arg.strip_prefix("--gamepad=")
                .and_then(|s| s.parse::<usize>().ok())
        })
        .unwrap_or(0);

    let sdl = match sdl3::init() {
        Ok(sdl) => sdl,
        Err(error) => {
            warn!("gamepad: SDL initialization failed: {error}");
            return None;
        }
    };
    let subsystem = match sdl.gamepad() {
        Ok(subsystem) => subsystem,
        Err(error) => {
            warn!("gamepad: SDL gamepad subsystem failed: {error}");
            return None;
        }
    };

    // Keep SDL alive even when no controller is present at launch. SDL may report devices
    // only after the window/event loop starts; poll_platform_gamepad re-enumerates on disconnect
    // and hot-plugs, using the same selected device index.
    let gamepad = match subsystem.gamepads() {
        Ok(ids) => match ids.get(gamepad_index).copied() {
            Some(id) => match subsystem.open(id) {
                Ok(gamepad) => {
                    info!("gamepad: opened controller at index {gamepad_index}");
                    Some(gamepad)
                }
                Err(error) => {
                    warn!("gamepad: opening controller failed: {error}");
                    None
                }
            },
            None => {
                info!("gamepad: no controller at index {gamepad_index}; waiting for hot-plug");
                None
            }
        },
        Err(error) => {
            warn!("gamepad: enumerate gamepads failed: {error}");
            None
        }
    };

    Some(SdlGamepad {
        gamepad,
        subsystem,
        gamepad_index,
        _sdl: sdl,
    })
}

#[cfg(target_os = "ios")]
pub(crate) struct SdlGamepad;

#[cfg(target_os = "ios")]
pub(crate) fn init_platform_gamepad() -> Option<SdlGamepad> {
    None
}

pub(crate) fn gamepad_slot(button: GamepadButton, modifier: u8) -> &'static str {
    const COMMANDS: [&str; 24] = [
        "GAMEPAD_SLOT_01",
        "GAMEPAD_SLOT_02",
        "GAMEPAD_SLOT_03",
        "GAMEPAD_SLOT_04",
        "GAMEPAD_SLOT_05",
        "GAMEPAD_SLOT_06",
        "GAMEPAD_SLOT_07",
        "GAMEPAD_SLOT_08",
        "GAMEPAD_SLOT_09",
        "GAMEPAD_SLOT_10",
        "GAMEPAD_SLOT_11",
        "GAMEPAD_SLOT_12",
        "GAMEPAD_SLOT_13",
        "GAMEPAD_SLOT_14",
        "GAMEPAD_SLOT_15",
        "GAMEPAD_SLOT_16",
        "GAMEPAD_SLOT_17",
        "GAMEPAD_SLOT_18",
        "GAMEPAD_SLOT_19",
        "GAMEPAD_SLOT_20",
        "GAMEPAD_SLOT_21",
        "GAMEPAD_SLOT_22",
        "GAMEPAD_SLOT_23",
        "GAMEPAD_SLOT_24",
    ];
    let base = match button {
        GamepadButton::A => 0,
        GamepadButton::B => 1,
        GamepadButton::X => 2,
        GamepadButton::Y => 3,
        GamepadButton::DPadUp => 4,
        GamepadButton::DPadDown => 5,
        GamepadButton::DPadLeft => 6,
        GamepadButton::DPadRight => 7,
    };
    COMMANDS[base + modifier.min(2) as usize * 8]
}

/// Runs the controller action-bar commands inside the normal binding pass. This keeps
/// BindingInput calls in the same frame and captures the L/R layer at button-down time.
fn gamepad_binding_events(
    snapshot: &GamepadSnapshot,
    bindings: &mut GamepadBindings,
) -> Vec<(&'static str, bool)> {
    let now = snapshot.current;
    let was = snapshot.previous;
    let mut events = Vec::new();

    if now.l != was.l || now.r != was.r || !bindings.modifier_initialized {
        let command = if now.l {
            "GAMEPAD_MODIFIER_L"
        } else if now.r {
            "GAMEPAD_MODIFIER_R"
        } else {
            "GAMEPAD_MODIFIER_CLEAR"
        };
        events.push((command, true));
        bindings.modifier_initialized = true;
    }

    let buttons = [
        (GamepadButton::A, now.a, was.a),
        (GamepadButton::B, now.b, was.b),
        (GamepadButton::X, now.x, was.x),
        (GamepadButton::Y, now.y, was.y),
        (GamepadButton::DPadUp, now.dpad_up, was.dpad_up),
        (GamepadButton::DPadDown, now.dpad_down, was.dpad_down),
        (GamepadButton::DPadLeft, now.dpad_left, was.dpad_left),
        (GamepadButton::DPadRight, now.dpad_right, was.dpad_right),
    ];
    let modifier = if now.l {
        1
    } else if now.r {
        2
    } else {
        0
    };

    for (button, pressed, was_pressed) in buttons {
        let trigger_owned = now.zl > 0.5
            && matches!(
                button,
                GamepadButton::A | GamepadButton::B | GamepadButton::X | GamepadButton::Y
            );
        if trigger_owned {
            if let Some(index) = bindings
                .latched
                .iter()
                .position(|(held, _)| *held == button)
            {
                let (_, command) = bindings.latched.remove(index);
                events.push((command, false));
            }
            continue;
        }

        if pressed && !was_pressed {
            let command = gamepad_slot(button, modifier);
            if !bindings.latched.iter().any(|(held, _)| *held == button) {
                events.push((command, true));
                bindings.latched.push((button, command));
            }
        } else if !pressed && was_pressed {
            if let Some(index) = bindings
                .latched
                .iter()
                .position(|(held, _)| *held == button)
            {
                let (_, command) = bindings.latched.remove(index);
                events.push((command, false));
            }
        }
    }

    events
}

/// Runs the controller action-bar commands inside the normal binding pass. This keeps
/// BindingInput calls in the same frame and captures the L/R layer at button-down time.
pub(crate) fn dispatch_action_bindings(
    script: Option<&mut UiScript>,
    snapshot: &GamepadSnapshot,
    bindings: &mut GamepadBindings,
) {
    let Some(script) = script else {
        return;
    };

    for (command, pressed) in gamepad_binding_events(snapshot, bindings) {
        if let Err(error) = script.execute_binding(command, pressed) {
            warn!("gamepad binding {command}: {error}");
        }
    }
}

/// The one-shot controller commands which already belong to WoW's binding or UI input paths.
pub(crate) fn dispatch_utility_bindings(
    script: Option<&mut UiScript>,
    snapshot: &GamepadSnapshot,
    dispatch: &crate::bindings::BindingDispatch,
) {
    let Some(script) = script else {
        return;
    };
    let now = snapshot.current;
    let was = snapshot.previous;

    if now.l3 && !was.l3 {
        if let Err(error) = script.execute_binding("TOGGLEAUTORUN", true) {
            warn!("gamepad binding TOGGLEAUTORUN: {error}");
        }
    }
    if now.plus && !was.plus {
        if let Err(error) = script.run("ToggleWorldMap()") {
            warn!("gamepad ToggleWorldMap(): {error}");
        }
    }

    let left_trigger = now.zl > 0.5;
    if left_trigger && now.x && !was.x {
        if let Err(error) = script.execute_binding("JUMP", true) {
            warn!("gamepad binding JUMP: {error}");
        }
    }
    if left_trigger && now.y && !was.y && !script.key_input("ESCAPE") {
        dispatch.dispatch_key(script, "ESCAPE");
    }
}

fn r3_target_mode(state: GamepadLuaState, in_combat: bool) -> NearestMode {
    if state.l {
        // LB + R3 → friendly.
        NearestMode::Friend
    } else if state.r {
        // RB + R3 → interactable.
        NearestMode::Interactable
    } else if in_combat {
        // R3 in combat → hostile.
        NearestMode::Enemy
    } else {
        // R3 out of combat → Smart.
        NearestMode::Smart
    }
}

/// R3 is edge-triggered and shares the same host and call stream as keyboard target cycles.
fn gamepad_r3_target_nearest(
    snapshot: Res<GamepadSnapshot>,
    self_q: Query<&crate::net::ObjectStore, With<crate::net::SelfPlayer>>,
    mut vm: crate::script_calls::VmWith<()>,
) {
    let state = snapshot.current;
    if !state.r3 || snapshot.previous.r3 {
        return;
    }
    let Ok(store) = self_q.single() else {
        return;
    };
    let in_combat = store.0.unit_flags() & (1 << 19) != 0;
    let mode = r3_target_mode(state, in_combat);
    vm.run(|script| {
        if let Some(script) = script {
            if let Err(error) = script.target_nearest_with_range(
                mode,
                false,
                Some(LEGACY_R3_TARGET_RANGE_YARDS),
            ) {
                warn!("gamepad R3 target cycle: {error}");
            }
        }
    });
}

/// RT interacts with the non-selectable target first, then the normal WoW selection.
fn gamepad_rt_interact(
    snapshot: Res<GamepadSnapshot>,
    selection: Res<crate::target::Selection>,
    mut interact: ResMut<crate::target::InteractTarget>,
    live_targets: Query<(), With<crate::net::Guid>>,
    mut interacts: MessageWriter<crate::target::Interact>,
) {
    interact.reconcile_selection(selection.guid);
    if interact
        .target
        .is_some_and(|target| live_targets.get(target).is_err())
    {
        interact.clear_target();
    }

    if snapshot.current.zr > 0.5 && snapshot.previous.zr <= 0.5 {
        if let Some(target) = interact.target.or(selection.target) {
            interacts.write(crate::target::Interact(target));
        }
    }
}

pub(crate) struct GamepadBridgePlugin;

impl Plugin for GamepadBridgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GamepadSnapshot>()
            .init_resource::<GamepadBindings>()
            .init_resource::<GamepadVirtualMouse>()
            .add_systems(
                Update,
                poll_platform_gamepad.before(crate::bindings::BindingHead),
            )
            .add_systems(
                Update,
                update_gamepad_virtual_mouse
                    .after(poll_platform_gamepad)
                    .after(crate::ui_script::collect_input_records)
                    .before(crate::ui_script::UiInputViews),
            )
            .add_systems(
                Update,
                inject_touch_input
                    .in_set(crate::ui_script::UiInput)
                    .after(poll_platform_gamepad)
                    .after(update_gamepad_virtual_mouse)
                    .after(crate::ui_script::collect_input_records)
                    .before(crate::ui_script::UiInputViews),
            )
            // R3's combat-mode decision reads the player's final packet-updated flags.
            .add_systems(
                Update,
                gamepad_r3_target_nearest
                    .in_set(crate::ui_script::UiFeed)
                    .after(poll_platform_gamepad),
            )
            .add_systems(
                Update,
                gamepad_rt_interact
                    .after(poll_platform_gamepad)
                    .before(crate::ui_script::UiInput),
            )
            .add_systems(
                Update,
                publish_gamepad_snapshot
                    .in_set(crate::ui_script::UiFeed)
                    .after(inject_touch_input),
            );
    }
}

#[cfg(not(target_os = "ios"))]
fn poll_platform_gamepad(
    gamepad: Option<NonSendMut<SdlGamepad>>,
    mut snapshot: ResMut<GamepadSnapshot>,
) {
    let Some(mut gamepad) = gamepad else {
        snapshot.previous = snapshot.current;
        snapshot.current = GamepadLuaState::default();
        return;
    };
    gamepad.subsystem.update();

    if gamepad.gamepad.as_ref().is_some_and(|pad| !pad.connected()) {
        info!("gamepad: controller disconnected");
        gamepad.gamepad = None;
    }

    if gamepad.gamepad.is_none() {
        match gamepad.subsystem.gamepads() {
            Ok(ids) => {
                if let Some(id) = ids.get(gamepad.gamepad_index).copied() {
                    match gamepad.subsystem.open(id) {
                        Ok(pad) => {
                            info!(
                                "gamepad: controller connected at index {}",
                                gamepad.gamepad_index
                            );
                            gamepad.gamepad = Some(pad);
                        }
                        Err(error) => {
                            warn!("gamepad: opening hot-plugged controller failed: {error}");
                        }
                    }
                }
            }
            Err(error) => warn!("gamepad: hot-plug enumeration failed: {error}"),
        }
    }

    let Some(pad) = gamepad.gamepad.as_ref() else {
        snapshot.previous = snapshot.current;
        snapshot.current = GamepadLuaState::default();
        return;
    };
    let left_x = normalize_axis(pad.axis(Axis::LeftX));
    let left_y = -normalize_axis(pad.axis(Axis::LeftY));
    let right_x = normalize_axis(pad.axis(Axis::RightX));
    let right_y = normalize_axis(pad.axis(Axis::RightY));

    snapshot.previous = snapshot.current;
    snapshot.current = GamepadLuaState {
        connected: true,
        a: pad.button(Button::South),
        b: pad.button(Button::East),
        x: pad.button(Button::West),
        y: pad.button(Button::North),
        l: pad.button(Button::LeftShoulder),
        r: pad.button(Button::RightShoulder),
        minus: pad.button(Button::Back),
        plus: pad.button(Button::Start),
        l3: pad.button(Button::LeftStick),
        r3: pad.button(Button::RightStick),
        dpad_up: pad.button(Button::DPadUp),
        dpad_down: pad.button(Button::DPadDown),
        dpad_left: pad.button(Button::DPadLeft),
        dpad_right: pad.button(Button::DPadRight),
        zl: normalize_trigger(pad.axis(Axis::TriggerLeft)),
        zr: normalize_trigger(pad.axis(Axis::TriggerRight)),
        left_x,
        left_y,
        right_x,
        right_y,
        ..Default::default()
    };
}

#[cfg(target_os = "ios")]
fn poll_platform_gamepad(mut snapshot: ResMut<GamepadSnapshot>) {
    use objc2_game_controller::GCController;

    let Some(controller) = (unsafe { GCController::current() }) else {
        snapshot.previous = snapshot.current;
        snapshot.current = GamepadLuaState::default();
        return;
    };
    let Some(gamepad) = (unsafe { controller.extendedGamepad() }) else {
        snapshot.previous = snapshot.current;
        snapshot.current = GamepadLuaState::default();
        return;
    };

    let mut state = GamepadLuaState {
        connected: true,
        ..Default::default()
    };
    unsafe {
        let left = gamepad.leftThumbstick();
        let right = gamepad.rightThumbstick();
        let dpad = gamepad.dpad();

        // Match the SDL convention: positive Y is forward/up.
        state.left_x = left.xAxis().value();
        state.left_y = left.yAxis().value();
        state.right_x = right.xAxis().value();
        state.right_y = -right.yAxis().value();

        state.a = gamepad.buttonA().isPressed();
        state.b = gamepad.buttonB().isPressed();
        state.x = gamepad.buttonX().isPressed();
        state.y = gamepad.buttonY().isPressed();
        state.l = gamepad.leftShoulder().isPressed();
        state.r = gamepad.rightShoulder().isPressed();
        state.zl = gamepad.leftTrigger().value();
        state.zr = gamepad.rightTrigger().value();

        // Apple names these Menu/Options. Keep the old fork's WoW mapping.
        state.minus = gamepad
            .buttonOptions()
            .is_some_and(|button| button.isPressed());
        state.plus = gamepad.buttonMenu().isPressed();
        state.l3 = gamepad
            .leftThumbstickButton()
            .is_some_and(|button| button.isPressed());
        state.r3 = gamepad
            .rightThumbstickButton()
            .is_some_and(|button| button.isPressed());

        state.dpad_up = dpad.up().isPressed();
        state.dpad_down = dpad.down().isPressed();
        state.dpad_left = dpad.left().isPressed();
        state.dpad_right = dpad.right().isPressed();
    }
    snapshot.previous = snapshot.current;
    snapshot.current = state;
}

fn publish_gamepad_snapshot(
    snapshot: Res<GamepadSnapshot>,
    mut vm: crate::script_calls::VmWith<()>,
) {
    vm.run(|script| {
        if let Some(script) = script {
            script.set_gamepad_state(snapshot.current);
        }
    });
}

#[cfg(not(target_os = "ios"))]
fn normalize_axis(value: i16) -> f32 {
    if value >= 0 {
        value as f32 / 32767.0
    } else {
        value as f32 / 32768.0
    }
}

#[cfg(not(target_os = "ios"))]
fn normalize_trigger(value: i16) -> f32 {
    value.max(0) as f32 / 32767.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sticks_use_the_radial_deadzone() {
        let snapshot = GamepadSnapshot {
            current: GamepadLuaState {
                left_x: 0.1,
                right_y: 0.1,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(snapshot.move_axis(), Vec2::ZERO);
        assert_eq!(snapshot.look_axis(), Vec2::ZERO);

        let snapshot = GamepadSnapshot {
            current: GamepadLuaState {
                left_y: 1.0,
                right_x: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(snapshot.move_axis(), Vec2::Y);
        assert_eq!(snapshot.look_axis(), Vec2::X);
    }

    #[test]
    fn left_trigger_routes_the_right_stick_to_the_virtual_mouse() {
        let snapshot = GamepadSnapshot {
            current: GamepadLuaState {
                connected: true,
                zl: 1.0,
                right_x: 0.75,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(snapshot.look_axis(), Vec2::ZERO);
        assert_ne!(snapshot.virtual_mouse_axis(), Vec2::ZERO);
    }

    #[test]
    fn virtual_mouse_motion_uses_ui_y_up_and_clamps_to_the_screen() {
        assert_eq!(
            advance_virtual_mouse(
                Vec2::new(10.0, 20.0),
                Vec2::new(1.0, 1.0),
                0.1,
                Vec2::splat(100.0)
            ),
            Vec2::new(80.0, 0.0)
        );
        assert_eq!(
            advance_virtual_mouse(
                Vec2::new(95.0, 95.0),
                Vec2::new(1.0, -1.0),
                0.1,
                Vec2::splat(100.0)
            ),
            Vec2::splat(100.0)
        );
    }

    #[test]
    fn camera_look_uses_the_legacy_down_positive_pitch_axis() {
        let up = GamepadSnapshot {
            current: GamepadLuaState {
                right_y: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let down = GamepadSnapshot {
            current: GamepadLuaState {
                right_y: -1.0,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(up.look_axis(), -Vec2::Y);
        assert_eq!(down.look_axis(), Vec2::Y);
        // The virtual pointer keeps its own UI-up convention.
        assert_eq!(up.virtual_mouse_axis(), Vec2::Y);
    }

    #[test]
    fn touch_stick_axes_are_centered_and_radially_clamped() {
        assert_eq!(
            touch_stick_axes(Vec2::new(132.0, 136.0), Vec2::new(132.0, 136.0), 63.0),
            Vec2::ZERO
        );
        assert_eq!(
            touch_stick_axes(Vec2::new(132.0, 199.0), Vec2::new(132.0, 136.0), 63.0),
            Vec2::Y
        );
        let clamped = touch_stick_axes(Vec2::new(195.0, 199.0), Vec2::new(132.0, 136.0), 63.0);
        assert!(
            (clamped.length() - 1.0).abs() < 1e-6,
            "radial clamp: {clamped:?}"
        );
    }

    #[test]
    fn a_hardware_pad_or_unlocked_stick_never_claims_touch_movement() {
        let centre = Vec2::new(132.0, 136.0);
        let point = Vec2::new(140.0, 140.0);
        assert!(touch_stick_claim_allowed(
            true, false, false, point, centre, 112.0
        ));
        assert!(!touch_stick_claim_allowed(
            false, false, false, point, centre, 112.0
        ));
        assert!(!touch_stick_claim_allowed(
            true, true, false, point, centre, 112.0
        ));
        assert!(!touch_stick_claim_allowed(
            true, false, true, point, centre, 112.0
        ));
    }

    #[test]
    fn touch_drag_requires_eight_screen_pixels() {
        assert!(!touch_look_started_drag(Vec2::ZERO, Vec2::new(7.9, 0.0)));
        assert!(touch_look_started_drag(Vec2::ZERO, Vec2::new(8.0, 0.0)));
    }

    #[test]
    fn r3_selects_the_expected_target_mode() {
        let base = GamepadLuaState::default();
        assert_eq!(r3_target_mode(base, true), NearestMode::Enemy);
        assert_eq!(r3_target_mode(base, false), NearestMode::Smart);
        assert_eq!(
            r3_target_mode(
                GamepadLuaState {
                    l: true,
                    r: true,
                    ..base
                },
                true
            ),
            NearestMode::Friend,
            "LB takes priority when both shoulder modifiers are held"
        );
        assert_eq!(
            r3_target_mode(GamepadLuaState { r: true, ..base }, false),
            NearestMode::Interactable
        );
    }

    #[test]
    fn action_slot_mapping_keeps_the_three_eight_button_layers() {
        let buttons = [
            GamepadButton::A,
            GamepadButton::B,
            GamepadButton::X,
            GamepadButton::Y,
            GamepadButton::DPadUp,
            GamepadButton::DPadDown,
            GamepadButton::DPadLeft,
            GamepadButton::DPadRight,
        ];
        for (index, button) in buttons.into_iter().enumerate() {
            assert_eq!(
                gamepad_slot(button, 0),
                format!("GAMEPAD_SLOT_{:02}", index + 1)
            );
            assert_eq!(
                gamepad_slot(button, 1),
                format!("GAMEPAD_SLOT_{:02}", index + 9)
            );
            assert_eq!(
                gamepad_slot(button, 2),
                format!("GAMEPAD_SLOT_{:02}", index + 17)
            );
        }
    }

    #[test]
    fn action_binding_release_uses_the_modifier_latched_at_press_time() {
        let mut bindings = GamepadBindings::default();
        let mut snapshot = GamepadSnapshot {
            current: GamepadLuaState {
                connected: true,
                a: true,
                l: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(
            gamepad_binding_events(&snapshot, &mut bindings),
            [("GAMEPAD_MODIFIER_L", true), ("GAMEPAD_SLOT_09", true)]
        );

        snapshot.previous = snapshot.current;
        snapshot.current = GamepadLuaState {
            connected: true,
            a: false,
            r: true,
            ..Default::default()
        };

        assert_eq!(
            gamepad_binding_events(&snapshot, &mut bindings),
            [("GAMEPAD_MODIFIER_R", true), ("GAMEPAD_SLOT_09", false)],
            "releasing A must release the L-layer command that was pressed, not the current R layer"
        );
    }

    #[test]
    fn left_trigger_releases_a_latched_face_button_without_restarting_it() {
        let mut bindings = GamepadBindings::default();
        let mut snapshot = GamepadSnapshot {
            current: GamepadLuaState {
                connected: true,
                a: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            gamepad_binding_events(&snapshot, &mut bindings),
            [("GAMEPAD_MODIFIER_CLEAR", true), ("GAMEPAD_SLOT_01", true)]
        );

        snapshot.previous = snapshot.current;
        snapshot.current.zl = 1.0;
        assert_eq!(
            gamepad_binding_events(&snapshot, &mut bindings),
            [("GAMEPAD_SLOT_01", false)],
            "LT takes ownership from an action already held"
        );

        snapshot.previous = snapshot.current;
        snapshot.current.a = false;
        assert!(
            gamepad_binding_events(&snapshot, &mut bindings).is_empty(),
            "the already-released latch must not emit a duplicate up edge"
        );
    }

    #[test]
    fn app_snapshot_is_published_to_lua() {
        let mut script = UiScript::new().expect("in-game script");
        let snapshot = GamepadSnapshot {
            current: GamepadLuaState {
                connected: true,
                a: true,
                left_x: 0.75,
                ..Default::default()
            },
            ..Default::default()
        };
        script.set_gamepad_state(snapshot.current);
        let connected: bool = script.eval("Gamepad_IsConnected()").unwrap();
        let a_down: bool = script.eval("Gamepad_IsButtonDown('A')").unwrap();
        let left_x: f32 = script.eval("Gamepad_GetAxis('LeftX')").unwrap();

        assert!(connected && a_down);
        assert_eq!(left_x, 0.75);
    }
}
