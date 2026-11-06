//! Platform gamepad polling and publication to FrameXML/Lua.
//!
//! SDL is initialized before Bevy starts its window/event loop. The app owns a
//! normalized snapshot; the Lua bridge only receives a copy of that snapshot.

use benilla_ui::script::{GamepadLuaState, UiScript};
use bevy::prelude::*;

#[cfg(not(target_os = "ios"))]
use sdl3::gamepad::{Axis, Button, Gamepad};

#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct GamepadSnapshot(pub(crate) GamepadLuaState);

#[cfg(not(target_os = "ios"))]
pub(crate) struct SdlGamepad {
    _sdl: sdl3::Sdl,
    gamepad: Gamepad,
}

#[cfg(not(target_os = "ios"))]
pub(crate) fn init_platform_gamepad() -> Option<SdlGamepad> {
    let gamepad_index = std::env::args()
        .skip(1)
        .find_map(|arg| arg.strip_prefix("--gamepad=").and_then(|s| s.parse::<usize>().ok()))
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
    let ids = match subsystem.gamepads() {
        Ok(ids) => ids,
        Err(error) => {
            warn!("gamepad: enumerate gamepads failed: {error}");
            return None;
        }
    };
    let Some(id) = ids.get(gamepad_index).copied() else {
        info!("gamepad: no controller at index {gamepad_index}");
        return None;
    };
    match subsystem.open(id) {
        Ok(gamepad) => {
            info!("gamepad: opened controller {}", gamepad.name());
            Some(SdlGamepad { _sdl: sdl, gamepad })
        }
        Err(error) => {
            warn!("gamepad: opening controller failed: {error}");
            None
        }
    }
}

#[cfg(target_os = "ios")]
pub(crate) fn init_platform_gamepad() -> Option<SdlGamepad> {
    None
}

pub(crate) struct GamepadBridgePlugin;

impl Plugin for GamepadBridgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GamepadSnapshot>().add_systems(
            Update,
            (
                poll_platform_gamepad,
                publish_gamepad_snapshot,
            )
                .chain()
                .before(crate::ui_script::UiTick),
        );
    }
}

#[cfg(not(target_os = "ios"))]
fn poll_platform_gamepad(
    gamepad: Option<NonSend<SdlGamepad>>,
    mut snapshot: ResMut<GamepadSnapshot>,
) {
    let Some(gamepad) = gamepad else {
        snapshot.0 = GamepadLuaState::default();
        return;
    };
    gamepad.gamepad.subsystem().update();
    let pad = &gamepad.gamepad;
    let left_x = normalize_axis(pad.axis(Axis::LeftX));
    let left_y = -normalize_axis(pad.axis(Axis::LeftY));
    let right_x = normalize_axis(pad.axis(Axis::RightX));
    let right_y = normalize_axis(pad.axis(Axis::RightY));

    snapshot.0 = GamepadLuaState {
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
    snapshot.0 = GamepadLuaState::default();
}

fn publish_gamepad_snapshot(
    snapshot: Res<GamepadSnapshot>,
    script: Option<NonSendMut<UiScript>>,
) {
    let Some(mut script) = script else {
        return;
    };
    script.set_gamepad_state(snapshot.0);
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
    fn app_snapshot_is_published_to_lua() {
        let mut app = App::new();
        app.insert_non_send_resource(UiScript::new().expect("in-game script"));
        app.insert_resource(GamepadSnapshot(GamepadLuaState {
            connected: true,
            a: true,
            left_x: 0.75,
            ..Default::default()
        }));

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
