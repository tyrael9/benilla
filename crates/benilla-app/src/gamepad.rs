use bevy::prelude::*;
#[cfg(not(target_os = "ios"))]
use sdl3::gamepad::{Axis, Button, Gamepad};
use benilla_ui::script::{NearestMode, ScriptCall, UiScript};
use crate::bindings::BindingDispatch;

// gamepad.rs

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GamepadButton {
    A,
    B,
    X,
    Y,

    L,
    R,

    Minus,
    Plus,

    L3,
    R3,

    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}


// ============================================================
// Gamepad state
// ============================================================

#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct GamepadState {
    pub connected: bool,
    pub move_stick: Vec2,
    pub look_stick: Vec2,

    // Face buttons
    pub a: bool,
    pub b: bool,
    pub x: bool,
    pub y: bool,

    pub prev_a: bool,
    pub prev_b: bool,
    pub prev_x: bool,
    pub prev_y: bool,

    // Shoulder buttons
    pub l: bool,
    pub r: bool,
    pub prev_l: bool,
    pub prev_r: bool,

    // Triggers
    pub zl: f32,
    pub zr: f32,

    pub prev_zl: f32,
    pub prev_zr: f32,

    // System buttons
    pub minus: bool,
    pub plus: bool,

    pub prev_minus: bool,
    pub prev_plus: bool,

    // Stick buttons
    pub l3: bool,
    pub r3: bool,

    pub prev_l3: bool,
    pub prev_r3: bool,

    // D-pad
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub dpad_left: bool,
    pub dpad_right: bool,

    pub prev_dpad_up: bool,
    pub prev_dpad_down: bool,
    pub prev_dpad_left: bool,
    pub prev_dpad_right: bool,

    pub virtual_mouse_x: f32,
    pub virtual_mouse_y: f32,
    pub virtual_mouse_initialized: bool,

    pub virtual_mouse_left_down: bool,
    pub virtual_mouse_right_down: bool,
}

impl GamepadState {
    pub(crate) fn move_axis(&self) -> Vec2 {
        apply_deadzone(self.move_stick, 0.15)
    }

    pub(crate) fn look_axis(&self) -> Vec2 {
        apply_deadzone(self.look_stick, 0.15)
    }

    pub(crate) fn button_down(&self, button: GamepadButton) -> bool {
        match button {
            GamepadButton::A => self.a,
            GamepadButton::B => self.b,
            GamepadButton::X => self.x,
            GamepadButton::Y => self.y,

            GamepadButton::L => self.l,
            GamepadButton::R => self.r,

            GamepadButton::Minus => self.minus,
            GamepadButton::Plus => self.plus,

            GamepadButton::L3 => self.l3,
            GamepadButton::R3 => self.r3,

            GamepadButton::DPadUp => self.dpad_up,
            GamepadButton::DPadDown => self.dpad_down,
            GamepadButton::DPadLeft => self.dpad_left,
            GamepadButton::DPadRight => self.dpad_right,
        }
    }

    pub(crate) fn button_was_down(&self, button: GamepadButton) -> bool {
        match button {
            GamepadButton::A => self.prev_a,
            GamepadButton::B => self.prev_b,
            GamepadButton::X => self.prev_x,
            GamepadButton::Y => self.prev_y,

            GamepadButton::L => self.prev_l,
            GamepadButton::R => self.prev_r,

            GamepadButton::Minus => self.prev_minus,
            GamepadButton::Plus => self.prev_plus,

            GamepadButton::L3 => self.prev_l3,
            GamepadButton::R3 => self.prev_r3,

            GamepadButton::DPadUp => self.prev_dpad_up,
            GamepadButton::DPadDown => self.prev_dpad_down,
            GamepadButton::DPadLeft => self.prev_dpad_left,
            GamepadButton::DPadRight => self.prev_dpad_right,
        }
    }

    pub(crate) fn button_pressed(&self, button: GamepadButton) -> bool {
        self.button_down(button) && !self.button_was_down(button)
    }

    pub(crate) fn button_released(&self, button: GamepadButton) -> bool {
        !self.button_down(button) && self.button_was_down(button)
    }

    pub(crate) fn lua_snapshot(&self) -> benilla_ui::script::GamepadLuaState {
        benilla_ui::script::GamepadLuaState {
            connected: self.connected,

            a: self.a,
            b: self.b,
            x: self.x,
            y: self.y,

            l: self.l,
            r: self.r,

            minus: self.minus,
            plus: self.plus,

            l3: self.l3,
            r3: self.r3,

            dpad_up: self.dpad_up,
            dpad_down: self.dpad_down,
            dpad_left: self.dpad_left,
            dpad_right: self.dpad_right,

            zl: self.zl,
            zr: self.zr,

            left_x: self.move_stick.x,
            left_y: self.move_stick.y,
            right_x: self.look_stick.x,
            right_y: self.look_stick.y,
        }
    }
}


// ============================================================
// SDL gamepad
//
// SDL objects are !Send + !Sync on macOS, so this must remain
// a NonSend resource in Bevy.
//
// IMPORTANT:
// SDL initialization must happen BEFORE App::run(), not from
// a Bevy Startup system. On macOS, SDL HID enumeration can
// interact with the event loop and cause winit's:
//
//   tried to handle event while another event is currently handled
//
// panic.
// ============================================================

#[cfg(not(target_os = "ios"))]
pub(crate) struct SdlGamepad {
    pub(crate) _sdl: sdl3::Sdl,
    pub(crate) gamepad: Gamepad,
}

#[cfg(target_os = "ios")]
pub(crate) struct SdlGamepad;


// ============================================================
// Initialize SDL + open the first gamepad.
//
// Call this BEFORE app.run().
// ============================================================

#[cfg(not(target_os = "ios"))]
pub(crate) fn init_sdl(gamepad_index: usize) -> Option<SdlGamepad> {
    println!("SDL: init");

    let sdl = match sdl3::init() {
        Ok(sdl) => { println!("SDL: sdl3::init OK"); sdl }
        Err(err) => { println!("SDL: init FAILED: {err}"); return None; }
    };
    let gamepad_subsystem = match sdl.gamepad() {
        Ok(gamepad) => { println!("SDL: gamepad subsystem OK"); gamepad }
        Err(err) => { println!("SDL: gamepad subsystem FAILED: {err}"); return None; }
    };
    let ids = match gamepad_subsystem.gamepads() {
        Ok(ids) => {
            println!("SDL: gamepads() OK, count = {}", ids.len());
            for id in &ids { println!("SDL GAMEPAD: {:?} {:?}", id, gamepad_subsystem.name_for_id(*id)); }
            ids
        }
        Err(err) => { println!("SDL: gamepads() FAILED: {err}"); return None; }
    };
    let Some(id) = ids.get(gamepad_index).copied() else {
        println!("SDL: NO GAMEPAD");
        return None;
    };
    let gamepad = match gamepad_subsystem.open(id) {
        Ok(gamepad) => { println!("SDL: gamepad opened OK"); gamepad }
        Err(err) => { println!("SDL: gamepad open FAILED: {err}"); return None; }
    };
    Some(SdlGamepad { _sdl: sdl, gamepad })
}

#[cfg(target_os = "ios")]
pub(crate) fn init_sdl(_gamepad_index: usize) -> Option<SdlGamepad> {
    None
}

// ============================================================
// Debug overlay marker
// ============================================================

#[derive(Component)]
struct GamepadDebugText;

fn gamepad_debug_enabled() -> bool {
    std::env::var("BENILLA_GAMEPAD_DEBUG").as_deref() == Ok("1")
}

#[cfg(target_os = "ios")]
#[derive(Component)]
struct IosDiagnosticsText;

#[cfg(target_os = "ios")]
#[derive(Resource, Default)]
struct IosDiagnosticsFps {
    elapsed: f32,
    frames: u32,
    fps: f32,
}

#[cfg(target_os = "ios")]
fn setup_ios_diagnostics(mut commands: Commands) {
    commands.spawn((
        Text::new("FPS --  BEVY --x--  SCREEN --x--"),
        TextFont {
            // This HUD is diagnostic-only; keep it small enough for the phone in landscape.
            font_size: 9.0,
            ..default()
        },
        TextColor(Color::WHITE),
        TextLayout::new_with_justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            top: px(4.0),
            left: px(0.0),
            right: px(0.0),
            width: percent(100.0),
            ..default()
        },
        IosDiagnosticsText,
    ));
}

#[cfg(target_os = "ios")]
fn update_ios_diagnostics(
    time: Res<Time>,
    mut fps: ResMut<IosDiagnosticsFps>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut text: Query<&mut Text, With<IosDiagnosticsText>>,
) {
    fps.elapsed += time.delta_secs();
    fps.frames += 1;

    if fps.elapsed < 0.25 {
        return;
    }

    fps.fps = fps.frames as f32 / fps.elapsed.max(0.001);
    fps.elapsed = 0.0;
    fps.frames = 0;

    let Ok(window) = window.single() else {
        return;
    };
    let Ok(mut text) = text.single_mut() else {
        return;
    };

    let physical_w = window.resolution.physical_width();
    let physical_h = window.resolution.physical_height();
    let scale = window.resolution.scale_factor();
    let logical_w = physical_w as f32 / scale;
    let logical_h = physical_h as f32 / scale;

    // UIKit reports bounds in points and nativeBounds in physical pixels.
    // nativeBounds is portrait-oriented even when the phone is in landscape.
    let (screen_w, screen_h, native_w, native_h, screen_scale, native_scale) = unsafe {
        use objc2_ui_kit::UIScreen;
        let Some(mtm) = objc2::main_thread_marker::MainThreadMarker::new() else {\n            return;\n        };\n        let screen = UIScreen::mainScreen(mtm);
        let bounds = screen.bounds();
        let native_bounds = screen.nativeBounds();
        (
            bounds.size.width,
            bounds.size.height,
            native_bounds.size.width,
            native_bounds.size.height,
            screen.scale(),
            screen.nativeScale(),
        )
    };

    *text = Text::new(format!(
        "FPS {:>4.1}  BEVY {:>4.0}x{:>4.0} / {:>4}x{:>4} @ {:.2}\nSCREEN {:>4.0}x{:>4.0}  NATIVE {:>4.0}x{:>4.0}  SCALE {:.2}/{:.2}",
        fps.fps,
        logical_w,
        logical_h,
        physical_w,
        physical_h,
        scale,
        screen_w,
        screen_h,
        native_w,
        native_h,
        screen_scale,
        native_scale,
    ));
}


// ============================================================
// Plugin
// ============================================================

pub(crate) struct GamepadPlugin;

impl Plugin for GamepadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GamepadState>()
            .add_systems(
                Startup,
                setup_gamepad_debug_overlay.run_if(gamepad_debug_enabled),
            )
            .add_systems(
                Update,
                poll_gamepad.before(crate::bindings::BindingSet),
            )
            .add_systems(
                Update,
                sync_gamepad_to_lua.after(poll_gamepad),
            )
            .add_systems(
                Update,
                gamepad_r3_target_nearest
                    .after(poll_gamepad)
                    .before(crate::script_calls::apply_script_calls),
            )
            .add_systems(
                Update,
                gamepad_utility_bindings
                    .after(poll_gamepad)
                    .before(crate::bindings::BindingSet),
            )
            .add_systems(Update, gamepad_rt_interact)
            .add_systems(
                Update,
                update_gamepad_debug_overlay.run_if(gamepad_debug_enabled),
            );

        #[cfg(target_os = "ios")]
        app.init_resource::<IosDiagnosticsFps>()
            .add_systems(Startup, setup_ios_diagnostics)
            .add_systems(Update, update_ios_diagnostics);
    }
}

// ============================================================
// Debug overlay
//
// Positioned at the top center of the screen.
// ============================================================

fn setup_gamepad_debug_overlay(mut commands: Commands) {
    commands.spawn((
        Text::new("GAMEPAD\nInitializing..."),

        TextFont {
            font_size: 18.0,
            ..default()
        },

        TextColor(Color::WHITE),

        TextLayout::new_with_justify(Justify::Center),

        Node {
            position_type: PositionType::Absolute,

            top: px(15.0),
            left: px(0.0),
            right: px(0.0),

            width: percent(100.0),

            ..default()
        },

        GamepadDebugText,
    ));
}


// ============================================================
// Poll SDL gamepad
// ============================================================

#[cfg(not(target_os = "ios"))]
fn poll_gamepad(
    sdl_gamepad: Option<NonSend<SdlGamepad>>,
    mut state: ResMut<GamepadState>,
) {
    let Some(sdl_gamepad) = sdl_gamepad else {
        state.connected = false;
        return;
    };

    state.connected = true;

    sdl_gamepad.gamepad.subsystem().update();

    let gamepad = &sdl_gamepad.gamepad;
    state.prev_a = state.a;
    state.prev_b = state.b;
    state.prev_x = state.x;
    state.prev_y = state.y;
    state.prev_dpad_up = state.dpad_up;
    state.prev_dpad_down = state.dpad_down;
    state.prev_dpad_left = state.dpad_left;
    state.prev_dpad_right = state.dpad_right;
    state.prev_l = state.l;
    state.prev_r = state.r;
    state.prev_minus = state.minus;
    state.prev_plus = state.plus;
    state.prev_l3 = state.l3;
    state.prev_r3 = state.r3;

    let left_x = gamepad.axis(Axis::LeftX);
    let left_y = gamepad.axis(Axis::LeftY);
    let right_x = gamepad.axis(Axis::RightX);
    let right_y = gamepad.axis(Axis::RightY);

    state.move_stick = Vec2::new(
        normalize_axis(left_x),
        -normalize_axis(left_y),
    );
    state.look_stick = Vec2::new(
        normalize_axis(right_x),
        normalize_axis(right_y),
    );

    state.a = gamepad.button(Button::South);
    state.b = gamepad.button(Button::East);
    state.x = gamepad.button(Button::West);
    state.y = gamepad.button(Button::North);

    state.l = gamepad.button(Button::LeftShoulder);
    state.r = gamepad.button(Button::RightShoulder);

    state.prev_zl = state.zl;
    state.zl = normalize_trigger(gamepad.axis(Axis::TriggerLeft));
    state.prev_zr = state.zr;
    state.zr = normalize_trigger(gamepad.axis(Axis::TriggerRight));

    state.minus = gamepad.button(Button::Back);
    state.plus = gamepad.button(Button::Start);
    state.l3 = gamepad.button(Button::LeftStick);
    state.r3 = gamepad.button(Button::RightStick);

    state.dpad_up = gamepad.button(Button::DPadUp);
    state.dpad_down = gamepad.button(Button::DPadDown);
    state.dpad_left = gamepad.button(Button::DPadLeft);
    state.dpad_right = gamepad.button(Button::DPadRight);
}

#[cfg(target_os = "ios")]
fn poll_gamepad(
    _sdl_gamepad: Option<NonSend<SdlGamepad>>,
    mut state: ResMut<GamepadState>,
) {
    use objc2_game_controller::GCController;

    let Some(controller) = (unsafe { GCController::current() }) else {
        state.connected = false;
        state.move_stick = Vec2::ZERO;
        state.look_stick = Vec2::ZERO;
        return;
    };

    let Some(gamepad) = (unsafe { controller.extendedGamepad() }) else {
        state.connected = false;
        state.move_stick = Vec2::ZERO;
        state.look_stick = Vec2::ZERO;
        return;
    };

    state.connected = true;

    state.prev_a = state.a;
    state.prev_b = state.b;
    state.prev_x = state.x;
    state.prev_y = state.y;
    state.prev_dpad_up = state.dpad_up;
    state.prev_dpad_down = state.dpad_down;
    state.prev_dpad_left = state.dpad_left;
    state.prev_dpad_right = state.dpad_right;
    state.prev_l = state.l;
    state.prev_r = state.r;
    state.prev_minus = state.minus;
    state.prev_plus = state.plus;
    state.prev_l3 = state.l3;
    state.prev_r3 = state.r3;

    unsafe {
        let left = gamepad.leftThumbstick();
        let right = gamepad.rightThumbstick();
        let dpad = gamepad.dpad();

        // GameController already returns normalized, deadzoned axis values.
        // Match the existing SDL convention: +Y means forward/up.
        state.move_stick = Vec2::new(left.xAxis().value(), left.yAxis().value());
        state.look_stick = Vec2::new(right.xAxis().value(), -right.yAxis().value());

        state.a = gamepad.buttonA().isPressed();
        state.b = gamepad.buttonB().isPressed();
        state.x = gamepad.buttonX().isPressed();
        state.y = gamepad.buttonY().isPressed();

        state.l = gamepad.leftShoulder().isPressed();
        state.r = gamepad.rightShoulder().isPressed();

        state.prev_zl = state.zl;
        state.prev_zr = state.zr;
        state.zl = gamepad.leftTrigger().value();
        state.zr = gamepad.rightTrigger().value();

        // Apple has Menu/Options rather than SDL's Back/Start pair.
        // Menu is the natural WoW "minus/back" equivalent; Options is "plus/map".
        state.minus = gamepad.buttonOptions().is_some_and(|button| button.isPressed());
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
}
fn gamepad_rt_interact(
    gamepad: Res<GamepadState>,
    selection: Res<crate::target::Selection>,
    interact: Res<crate::target::InteractTarget>,
    mut interacts: MessageWriter<crate::target::Interact>,
) {
    if gamepad.zr > 0.5 && gamepad.prev_zr <= 0.5 {
        let target = interact.target.or(selection.target);

        if let Some(target) = target {
            interacts.write(crate::target::Interact(target));
        }
    }
}

fn gamepad_utility_bindings(
    gamepad: Res<GamepadState>,
    script: Option<NonSendMut<UiScript>>,
    dispatch: Res<BindingDispatch>,
) {
    let Some(mut script) = script else {
        return;
    };

    // ========================================================
    // L3 → Auto Run
    //
    // One-shot. ToggleAutoRun() is already part of the normal
    // binding/input pipeline, so do not touch Player.autorun
    // directly.
    // ========================================================

    if gamepad.l3 && !gamepad.prev_l3 {
        if let Err(e) = script.execute_binding("TOGGLEAUTORUN", true) {
            warn!("gamepad: TOGGLEAUTORUN: {e}");
        }
    }

    // ========================================================
    // + / Start → World Map
    //
    // Use the existing WoW Lua path so the map button state,
    // WorldMapFrame, zoom state, etc. remain consistent.
    // ========================================================

    if gamepad.plus && !gamepad.prev_plus {
        if let Err(e) = script.run("ToggleWorldMap()") {
            warn!("gamepad: ToggleWorldMap(): {e}");
        }
    }

    let lt = gamepad.zl > 0.5;
    let prev_lt = gamepad.prev_zl > 0.5;

    if lt && !prev_lt {
        // LT itself is only a modifier. Do not fire anything
        // merely by pressing LT.
    }

    // ========================================================
    // LT + X → Jump
    //
    // ZL is the modifier; X is the action.
    // One-shot on X press.
    // ========================================================

    if lt && gamepad.x && !gamepad.prev_x {
        if let Err(e) = script.execute_binding("JUMP", true) {
            warn!("gamepad: JUMP: {e}");
        }
    }

    // ========================================================
    // LT + Y → Escape / Back
    //
    // Actual Escape handling is UI-level and should use the
    // existing Escape input path.
    // ========================================================

    if lt && gamepad.y && !gamepad.prev_y {
        // Same path as a physical Escape key:
        // 1. Let the UI consume ESCAPE first.
        // 2. If nobody consumes it, dispatch the ESCAPE binding.
        if !script.key_input("ESCAPE") {
            dispatch.dispatch_key(&mut script, "ESCAPE");
        }
    }
}

fn gamepad_r3_target_nearest(
    gamepad: Res<GamepadState>,
    self_q: Query<&crate::net::ObjectStore, With<crate::net::SelfPlayer>>,
    script: Option<NonSendMut<UiScript>>,
) {
    if !gamepad.r3 || gamepad.prev_r3 {
        return;
    }

    let Some(mut script) = script else {
        return;
    };

    let in_combat = self_q
        .single()
        .ok()
        .is_some_and(|store| {
            store.0.unit_flags() & crate::player::UNIT_FLAG_IN_COMBAT != 0
        });

    let mode = if gamepad.l {
        // LB + R3 → friendly
        NearestMode::Friend
    } else if gamepad.r {
        // RB + R3 → interactable
        NearestMode::Interactable
    } else if in_combat {
        // R3 in combat → hostile
        NearestMode::Enemy
    } else {
        // R3 out of combat → smart hostile/interactable
        NearestMode::Smart
    };

    script.push_script_call(ScriptCall::TargetNearest {
        mode,
        reverse: false,
    });
}

fn sync_gamepad_to_lua(
    gamepad: Res<GamepadState>,
    script: Option<NonSendMut<UiScript>>,
) {
    let Some(mut script) = script else {
        return;
    };

    script.set_gamepad_state(gamepad.lua_snapshot());
}


// ============================================================
// Update debug overlay
// ============================================================

fn update_gamepad_debug_overlay(
    state: Res<GamepadState>,
    mut query: Query<&mut Text, With<GamepadDebugText>>,
) {
    let Ok(mut text) = query.single_mut() else {
        return;
    };

    let move_stick = state.move_stick;
    let look_stick = state.look_stick;

    *text = Text::new(format!(
        "\
GAMEPAD

LEFT  {:>6.3} {:>6.3}       RIGHT {:>6.3} {:>6.3}

A {}   B {}   X {}   Y {}
L {}   R {}   ZL {:>4.2}   ZR {:>4.2}
- {}   + {}   L3 {}   R3 {}

D-PAD  ↑ {}   ↓ {}   ← {}   → {}
",
        move_stick.x,
        move_stick.y,

        look_stick.x,
        look_stick.y,

        button_char(state.a),
        button_char(state.b),
        button_char(state.x),
        button_char(state.y),

        button_char(state.l),
        button_char(state.r),

        state.zl,
        state.zr,

        button_char(state.minus),
        button_char(state.plus),

        button_char(state.l3),
        button_char(state.r3),

        button_char(state.dpad_up),
        button_char(state.dpad_down),
        button_char(state.dpad_left),
        button_char(state.dpad_right),
    ));
}


// ============================================================
// Button display helper
// ============================================================

fn button_char(pressed: bool) -> &'static str {
    if pressed {
        "[X]"
    } else {
        "[ ]"
    }
}


// ============================================================
// SDL i16 axis → [-1.0, +1.0]
// ============================================================

fn normalize_axis(value: i16) -> f32 {
    if value >= 0 {
        value as f32 / 32767.0
    } else {
        value as f32 / 32768.0
    }
}


// ============================================================
// SDL trigger → [0.0, 1.0]
// ============================================================

fn normalize_trigger(value: i16) -> f32 {
    value.max(0) as f32 / 32767.0
}


// ============================================================
// Radial deadzone
// ============================================================

fn apply_deadzone(v: Vec2, deadzone: f32) -> Vec2 {
    let magnitude = v.length();

    if magnitude <= deadzone {
        return Vec2::ZERO;
    }

    let normalized = v / magnitude;

    let magnitude = ((magnitude - deadzone) / (1.0 - deadzone))
        .clamp(0.0, 1.0);

    normalized * magnitude
}