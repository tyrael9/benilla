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
            |lua, (center_x, center_y, radius, travel, enabled, unlocked):
                (f32, f32, f32, f32, bool, bool)| {
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