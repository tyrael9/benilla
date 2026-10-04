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

                _ => 0.0,
            })
        })?,
    )?;

    Ok(())
}