use mlua::Lua;

use super::Model;

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    g.set(
        "Gamepad_IsConnected",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.gamepad.connected)
        })?,
    )?;

    g.set(
        "Gamepad_IsButtonDown",
        lua.create_function(|lua, button: String| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");

            Ok(match button.as_str() {
                "A" => model.gamepad.a,
                "B" => model.gamepad.b,
                "X" => model.gamepad.x,
                "Y" => model.gamepad.y,

                "L" => model.gamepad.l,
                "R" => model.gamepad.r,

                "Minus" => model.gamepad.minus,
                "Plus" => model.gamepad.plus,

                "L3" => model.gamepad.l3,
                "R3" => model.gamepad.r3,

                "DPadUp" => model.gamepad.dpad_up,
                "DPadDown" => model.gamepad.dpad_down,
                "DPadLeft" => model.gamepad.dpad_left,
                "DPadRight" => model.gamepad.dpad_right,

                _ => false,
            })
        })?,
    )?;

    g.set(
        "Gamepad_GetAxis",
        lua.create_function(|lua, axis: String| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");

            Ok(match axis.as_str() {
                "ZL" => model.gamepad.zl,
                "ZR" => model.gamepad.zr,

                "LeftX" => model.gamepad.left_x,
                "LeftY" => model.gamepad.left_y,
                "RightX" => model.gamepad.right_x,
                "RightY" => model.gamepad.right_y,
                "TouchMoveX" => model.gamepad.touch_move_x,
                "TouchMoveY" => model.gamepad.touch_move_y,

                _ => 0.0,
            })
        })?,
    )?;

    g.set(
        "Gamepad_IsTouchDevice",
        lua.create_function(|_lua, ()| Ok(cfg!(target_os = "ios")))?,
    )?;

    g.set(
        "Gamepad_SetTouchStick",
        lua.create_function(
            |lua,
             (center_x, center_y, radius, travel, enabled, unlocked): (
                f32,
                f32,
                f32,
                f32,
                bool,
                bool,
            )| {
                let mut model = lua.app_data_mut::<Model>().expect("model app_data");
                if center_x.is_finite() {
                    model.touch_stick_center_x = center_x;
                }
                if center_y.is_finite() {
                    model.touch_stick_center_y = center_y;
                }
                if radius.is_finite() {
                    model.touch_stick_radius = radius.clamp(48.0, 180.0);
                }
                if travel.is_finite() {
                    model.touch_stick_travel = travel.clamp(24.0, 160.0);
                }
                model.touch_stick_enabled = enabled;
                model.touch_stick_unlocked = unlocked;
                Ok(())
            },
        )?,
    )?;

    g.set(
        "Gamepad_GetTouchStick",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok((
                model.touch_stick_center_x,
                model.touch_stick_center_y,
                model.touch_stick_radius,
                model.touch_stick_enabled,
                model.touch_stick_unlocked,
            ))
        })?,
    )?;

    g.set(
        "Gamepad_GetTouchStickValue",
        lua.create_function(|lua, key: String| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(match key.as_str() {
                "X" => model.touch_stick_center_x,
                "Y" => model.touch_stick_center_y,
                "Radius" => model.touch_stick_radius,
                "Travel" => model.touch_stick_travel,
                _ => 0.0,
            })
        })?,
    )?;

    g.set(
        "Gamepad_IsTouchStickEnabled",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.touch_stick_enabled)
        })?,
    )?;

    g.set(
        "Gamepad_IsTouchStickUnlocked",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.touch_stick_unlocked)
        })?,
    )?;

    g.set(
        "Gamepad_IsTouchStickActive",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.gamepad.touch_move_active)
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{GamepadLuaState, UiScript};

    #[test]
    fn lua_reads_the_latest_platform_controller_snapshot() {
        let mut script = UiScript::new().expect("in-game script");
        script.set_gamepad_state(GamepadLuaState {
            connected: true,
            a: true,
            r3: true,
            dpad_left: true,
            zl: 0.75,
            left_x: -0.5,
            right_y: 0.25,
            touch_move_x: 0.6,
            touch_move_active: true,
            ..Default::default()
        });

        let connected: bool = script.eval("Gamepad_IsConnected()").unwrap();
        let a_down: bool = script.eval("Gamepad_IsButtonDown('A')").unwrap();
        let r3_down: bool = script.eval("Gamepad_IsButtonDown('R3')").unwrap();
        let dpad_left: bool = script.eval("Gamepad_IsButtonDown('DPadLeft')").unwrap();
        let zl: f32 = script.eval("Gamepad_GetAxis('ZL')").unwrap();
        let left_x: f32 = script.eval("Gamepad_GetAxis('LeftX')").unwrap();
        let right_y: f32 = script.eval("Gamepad_GetAxis('RightY')").unwrap();
        let touch_x: f32 = script.eval("Gamepad_GetAxis('TouchMoveX')").unwrap();
        let touch_active: bool = script.eval("Gamepad_IsTouchStickActive()").unwrap();

        assert!(connected && a_down && r3_down && dpad_left && touch_active);
        assert_eq!(zl, 0.75);
        assert_eq!(left_x, -0.5);
        assert_eq!(right_y, 0.25);
        assert_eq!(touch_x, 0.6);
    }

    #[test]
    fn mobile_stick_settings_round_trip_and_clamp() {
        let script = UiScript::new().expect("in-game script");
        script
            .run("Gamepad_SetTouchStick(120, 240, 500, 2, true, true)")
            .unwrap();

        let (x, y, radius, enabled, unlocked): (f32, f32, f32, bool, bool) =
            script.eval("Gamepad_GetTouchStick()").unwrap();
        let travel: f32 = script.eval("Gamepad_GetTouchStickValue('Travel')").unwrap();

        assert_eq!((x, y, radius, travel), (120.0, 240.0, 180.0, 24.0));
        assert!(enabled && unlocked);
        let is_touch: bool = script.eval("Gamepad_IsTouchDevice()").unwrap();
        assert_eq!(is_touch, cfg!(target_os = "ios"));
    }
}
