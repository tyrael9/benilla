//! Publishes the platform controller snapshot to the active Lua VM.
//!
//! Device polling is deliberately kept out of this bridge. SDL3 (desktop) and
//! GameController (iOS) adapters will write the same normalized snapshot here;
//! this system copies it into UiScript before the FrameXML tick.

use benilla_ui::script::{GamepadLuaState, UiScript};
use bevy::prelude::*;

/// Latest normalized controller state, owned by the app rather than by Lua.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct GamepadSnapshot(pub(crate) GamepadLuaState);

pub(crate) struct GamepadBridgePlugin;

impl Plugin for GamepadBridgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GamepadSnapshot>().add_systems(
            Update,
            publish_gamepad_snapshot.before(crate::ui_script::UiTick),
        );
    }
}

fn publish_gamepad_snapshot(snapshot: Res<GamepadSnapshot>, script: Option<NonSendMut<UiScript>>) {
    let Some(mut script) = script else {
        return;
    };

    script.set_gamepad_state(snapshot.0);
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
