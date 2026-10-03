use bevy::prelude::*;
use sdl3::gamepad::{Axis, Button, Gamepad};
use benilla_ui::script::{NearestMode, ScriptCall, UiScript};


// ============================================================
// Gamepad state
// ============================================================

#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct GamepadState {
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

    // Triggers
    pub zl: f32,
    pub zr: f32,

    pub prev_zr: f32,

    // System buttons
    pub minus: bool,
    pub plus: bool,

    // Stick buttons
    pub l3: bool,
    pub r3: bool,

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

pub(crate) struct SdlGamepad {
    pub(crate) _sdl: sdl3::Sdl,
    pub(crate) gamepad: Gamepad,
}


// ============================================================
// Initialize SDL + open the first gamepad.
//
// Call this BEFORE app.run().
// ============================================================

pub(crate) fn init_sdl() -> Option<SdlGamepad> {
    println!("SDL: init");

    let sdl = match sdl3::init() {
        Ok(sdl) => {
            println!("SDL: sdl3::init OK");
            sdl
        }
        Err(err) => {
            println!("SDL: init FAILED: {err}");
            return None;
        }
    };

    println!("SDL: getting gamepad subsystem");

    let gamepad_subsystem = match sdl.gamepad() {
        Ok(gamepad) => {
            println!("SDL: gamepad subsystem OK");
            gamepad
        }
        Err(err) => {
            println!("SDL: gamepad subsystem FAILED: {err}");
            return None;
        }
    };

    println!("SDL: enumerating gamepads");

    let ids = match gamepad_subsystem.gamepads() {
        Ok(ids) => {
            println!("SDL: gamepads() OK, count = {}", ids.len());

            for id in &ids {
                println!(
                    "SDL GAMEPAD: {:?} {:?}",
                    id,
                    gamepad_subsystem.name_for_id(*id)
                );
            }

            ids
        }
        Err(err) => {
            println!("SDL: gamepads() FAILED: {err}");
            return None;
        }
    };

    let Some(id) = ids.first().copied() else {
        println!("SDL: NO GAMEPAD");
        return None;
    };

    println!("SDL: opening gamepad {:?}", id);

    let gamepad = match gamepad_subsystem.open(id) {
        Ok(gamepad) => {
            println!("SDL: gamepad opened OK");
            gamepad
        }
        Err(err) => {
            println!("SDL: gamepad open FAILED: {err}");
            return None;
        }
    };

    Some(SdlGamepad {
        _sdl: sdl,
        gamepad,
    })
}


// ============================================================
// Debug overlay marker
// ============================================================

#[derive(Component)]
struct GamepadDebugText;

fn gamepad_debug_enabled() -> bool {
    std::env::var("BENILLA_GAMEPAD_DEBUG").as_deref() == Ok("1")
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
                gamepad_r3_target_nearest_interactable
                    .after(poll_gamepad)
                    .before(crate::script_calls::apply_script_calls),
            )
            .add_systems(Update, gamepad_rt_interact)
            .add_systems(
                Update, 
                update_gamepad_debug_overlay.run_if(gamepad_debug_enabled),
            );
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

fn poll_gamepad(
    sdl_gamepad: Option<NonSend<SdlGamepad>>,
    mut state: ResMut<GamepadState>,
) {
    let Some(sdl_gamepad) = sdl_gamepad else {
        return;
    };

    // SDL is not running its own event loop.
    //
    // Explicitly update the gamepad state once per frame.
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


    // --------------------------------------------------------
    // Analog sticks
    // --------------------------------------------------------

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


    // --------------------------------------------------------
    // Face buttons
    //
    // SDL uses the standard gamepad layout:
    //
    // South = A
    // East  = B
    // West  = X
    // North = Y
    // --------------------------------------------------------

    state.a = gamepad.button(Button::South);
    state.b = gamepad.button(Button::East);
    state.x = gamepad.button(Button::West);
    state.y = gamepad.button(Button::North);


    // --------------------------------------------------------
    // Shoulder buttons
    // --------------------------------------------------------

    state.l = gamepad.button(Button::LeftShoulder);
    state.r = gamepad.button(Button::RightShoulder);


    // --------------------------------------------------------
    // Triggers
    //
    // ZL / ZR are analog axes in SDL's Gamepad API.
    // --------------------------------------------------------

    state.zl = normalize_trigger(
        gamepad.axis(Axis::TriggerLeft),
    );

    state.prev_zr = state.zr;
    state.zr = normalize_trigger(
        gamepad.axis(Axis::TriggerRight),
    );


    // --------------------------------------------------------
    // System buttons
    // --------------------------------------------------------

    state.minus = gamepad.button(Button::Back);
    state.plus = gamepad.button(Button::Start);


    // --------------------------------------------------------
    // Stick buttons
    // --------------------------------------------------------
    state.prev_r3 = state.r3;
    state.l3 = gamepad.button(Button::LeftStick);
    state.r3 = gamepad.button(Button::RightStick);


    // --------------------------------------------------------
    // D-pad
    // --------------------------------------------------------

    state.dpad_up = gamepad.button(Button::DPadUp);
    state.dpad_down = gamepad.button(Button::DPadDown);
    state.dpad_left = gamepad.button(Button::DPadLeft);
    state.dpad_right = gamepad.button(Button::DPadRight);
}

fn gamepad_rt_interact(
    gamepad: Res<GamepadState>,
    selection: Res<crate::target::Selection>,
    mut interacts: MessageWriter<crate::target::Interact>,
) {
    if gamepad.zr > 0.5 && gamepad.prev_zr <= 0.5 {
        if let Some(target) = selection.target {
            interacts.write(crate::target::Interact(target));
        }
    }
}

fn gamepad_r3_target_nearest_interactable(
    gamepad: Res<GamepadState>,
    script: Option<NonSendMut<UiScript>>,
) {
    if !gamepad.r3 || gamepad.prev_r3 {
        return;
    }

    let Some(mut script) = script else {
        return;
    };

    script.push_script_call(ScriptCall::TargetNearest {
        mode: NearestMode::Interactable,
        reverse: false,
    });
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