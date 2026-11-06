//! Platform gamepad polling and publication to FrameXML/Lua.
//!
//! SDL is initialized before Bevy starts its window/event loop. The app owns a
//! normalized snapshot; the Lua bridge only receives a copy of that snapshot.

use benilla_ui::script::{GamepadLuaState, NearestMode, UiScript};
use bevy::prelude::*;

#[cfg(not(target_os = "ios"))]
use sdl3::gamepad::{Axis, Button, Gamepad};

#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct GamepadSnapshot {
    pub(crate) current: GamepadLuaState,
    pub(crate) previous: GamepadLuaState,
}

impl GamepadSnapshot {
    /// The old controller's radial deadzone: a centered stick is zero, then the remaining
    /// magnitude is remapped to the full range without changing its direction.
    pub(crate) fn move_axis(&self) -> Vec2 {
        apply_deadzone(Vec2::new(self.current.left_x, self.current.left_y), 0.15)
    }

    pub(crate) fn look_axis(&self) -> Vec2 {
        apply_deadzone(Vec2::new(self.current.right_x, self.current.right_y), 0.15)
    }
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
pub(crate) fn dispatch_action_bindings(
    script: Option<&mut UiScript>,
    snapshot: &GamepadSnapshot,
    bindings: &mut GamepadBindings,
) {
    let Some(script) = script else {
        return;
    };
    let now = snapshot.current;
    let was = snapshot.previous;

    if now.l != was.l || now.r != was.r || !bindings.modifier_initialized {
        let command = if now.l {
            "GAMEPAD_MODIFIER_L"
        } else if now.r {
            "GAMEPAD_MODIFIER_R"
        } else {
            "GAMEPAD_MODIFIER_CLEAR"
        };
        if let Err(error) = script.execute_binding(command, true) {
            warn!("gamepad binding {command}: {error}");
        }
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
                if let Err(error) = script.execute_binding(command, false) {
                    warn!("gamepad binding {command} release: {error}");
                }
            }
            continue;
        }

        if pressed && !was_pressed {
            let command = gamepad_slot(button, modifier);
            if !bindings.latched.iter().any(|(held, _)| *held == button) {
                if let Err(error) = script.execute_binding(command, true) {
                    warn!("gamepad binding {command}: {error}");
                }
                bindings.latched.push((button, command));
            }
        } else if !pressed && was_pressed {
            if let Some(index) = bindings
                .latched
                .iter()
                .position(|(held, _)| *held == button)
            {
                let (_, command) = bindings.latched.remove(index);
                if let Err(error) = script.execute_binding(command, false) {
                    warn!("gamepad binding {command} release: {error}");
                }
            }
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
            if let Err(error) = script.target_nearest(mode, false) {
                warn!("gamepad R3 target cycle: {error}");
            }
        }
    });
}

/// RT interacts with the non-selectable target first, then the normal WoW selection.
fn gamepad_rt_interact(
    snapshot: Res<GamepadSnapshot>,
    selection: Res<crate::target::Selection>,
    interact: Res<crate::target::InteractTarget>,
    mut interacts: MessageWriter<crate::target::Interact>,
) {
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
            .add_systems(
                Update,
                poll_platform_gamepad.before(crate::bindings::BindingHead),
            )
            .add_systems(
                Update,
                (gamepad_r3_target_nearest, gamepad_rt_interact)
                    .after(poll_platform_gamepad)
                    .before(crate::ui_script::UiInput),
            )
            .add_systems(
                Update,
                publish_gamepad_snapshot.before(crate::ui_script::UiTick),
            );
    }
}

#[cfg(not(target_os = "ios"))]
fn poll_platform_gamepad(
    mut gamepad: Option<NonSendMut<SdlGamepad>>,
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

fn publish_gamepad_snapshot(snapshot: Res<GamepadSnapshot>, script: Option<NonSendMut<UiScript>>) {
    let Some(mut script) = script else {
        return;
    };
    script.set_gamepad_state(snapshot.current);
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
    use bevy::ecs::system::RunSystemOnce;

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
    fn app_snapshot_is_published_to_lua() {
        let mut app = App::new();
        app.insert_non_send_resource(UiScript::new().expect("in-game script"));
        app.insert_resource(GamepadSnapshot {
            current: GamepadLuaState {
                connected: true,
                a: true,
                left_x: 0.75,
                ..Default::default()
            },
            ..Default::default()
        });

        app.world_mut()
            .run_system_once(publish_gamepad_snapshot)
            .expect("publish controller snapshot");

        let script = app.world().non_send_resource::<UiScript>();
        let connected: bool = script.eval("Gamepad_IsConnected()").unwrap();
        let a_down: bool = script.eval("Gamepad_IsButtonDown('A')").unwrap();
        let left_x: f32 = script.eval("Gamepad_GetAxis('LeftX')").unwrap();

        assert!(connected && a_down);
        assert_eq!(left_x, 0.75);
    }
}
