//! The UI scale: `UIParent`'s own frame scale, never the screen's. The screen root keeps scale 1
//! and is 768 units tall at every window size; `uiScale` reaches only `UIParent`'s subtree,
//! through the worker Lua's `SetScale` calls (`0x494550` → `0x76ac10` on the frame named
//! `"UIParent"`, looked up at `0x49006a`), so a frame with no parent, or under `WorldFrame`, draws
//! at scale 1.

use super::{Model, UiScript};

/// `SetUiScale`'s floor (`0x494550`: `[0x804584]`, below which `0x494563` stores 0.64).
pub(crate) const UI_SCALE_FLOOR: f32 = 0.64;

/// `GetUiScale` (`0x494590`): `UIParent`'s effective scale (`[UIParent+0x7c]`), else 1.0 when there
/// is no `UIParent`. `GetScreenWidth`/`GetScreenHeight` (`0x48b480`/`0x48b4d0`) divide the root by
/// it, and nothing else reads it. A zero scale reads 1, as the reference's writer `0x768360`
/// refuses one.
pub(crate) fn ui_parent_scale(model: &Model) -> f32 {
    model
        .arena
        .lookup("UIParent")
        .map_or(1.0, |h| super::object::eff_scale(model, h))
}

/// The display handler's automatic scale cap: below a 4:3 aspect `a`, it is capped at `0.75·a`.
/// A user-selected `uiScale` deliberately bypasses this cap so narrow split-screen windows remain
/// adjustable; the automatic fallback retains the reference's aspect-ratio behavior.
pub(crate) fn capped_ui_scale(ui_scale: f32, aspect: f32) -> f32 {
    if aspect * 3.0 < 4.0 {
        ui_scale.min(aspect * 0.75)
    } else {
        ui_scale
    }
}

/// `SetUiScale`'s floor (`0x494550`): above [`UI_SCALE_FLOOR`] the value stands, else (NaN
/// included) the floor.
pub(crate) fn floored_ui_scale(v: f32) -> f32 {
    if v > UI_SCALE_FLOOR {
        v
    } else {
        UI_SCALE_FLOOR
    }
}

/// What a user-selected `uiScale` applies: the dial value, subject only to `SetUiScale`'s floor.
/// The split-window fork intentionally skips the reference's narrow-aspect cap for manual values.
pub(crate) fn manual_ui_scale(ui_scale: f32) -> f32 {
    floored_ui_scale(ui_scale)
}

/// The automatic scale `0x492f70` computes for a display `h` tall at the display aspect `aspect`
/// (`0x4901e0`, read at `0x492fb4`): 1.0, or `768/h` above 768 tall (`0x492fbc cmp esi,0x300`),
/// under the same narrow-aspect cap, then raised to 0.9 unless above it (`0x493005`–`0x493012`).
pub(crate) fn auto_ui_scale(h: u32, aspect: f32) -> f32 {
    let mut v = if h > 768 { 768.0 / h as f32 } else { 1.0 };
    if h > 0 {
        v = capped_ui_scale(v, aspect);
    }
    if v > 0.9 {
        v
    } else {
        0.9
    }
}

impl UiScript {
    /// The host's `uiScale`, compared first like [`Self::set_screen_size`]: a new manual value
    /// applies at once to `UIParent`, floored at 0.64 but not capped by the window's aspect ratio.
    /// The same value again does nothing, so an addon's own `UIParent:SetScale` holds until the
    /// next change.
    pub fn set_ui_scale(&mut self, ui_scale: f32) -> bool {
        let applied = {
            let mut model = self.model_mut();
            if model.ui_scale_request == Some(ui_scale) {
                return false;
            }
            model.ui_scale_request = Some(ui_scale);
            model
                .arena
                .lookup("UIParent")
                .map(|h| (h, manual_ui_scale(ui_scale)))
        };
        let Some((ui_parent, v)) = applied else {
            return false;
        };
        self.model_mut().set_frame_scale(ui_parent, v, true);
        true
    }

    /// Record `ui_scale` without applying it, then apply it through the display handler, as the UI
    /// load does once its files and `VARIABLES_LOADED` are done (`0x49010e` → `0x492e90`): the
    /// CVar's own callback found no `UIParent` when it fired at registration.
    pub fn seat_ui_scale(
        &mut self,
        ui_scale: f32,
        display: (u32, u32),
        auto_cache: &mut Option<(u32, u32)>,
    ) -> bool {
        self.model_mut().ui_scale_request = Some(ui_scale);
        self.apply_ui_scale(display, auto_cache)
    }

    /// The display handler `0x492e90` on the last [`Self::set_ui_scale`], for the load and a
    /// display change. At or above the 0.64 floor it reapplies the manual dial value directly; below
    /// the floor it falls to the automatic scale `0x492f70(w, h, force = 0)` ([`auto_ui_scale`]),
    /// which retains the narrow-aspect cap and does nothing when the display is the one it last ran
    /// for (`auto_cache`, the reference's `[0xb4e304]`/`[0xb4e308]`, which outlive the UI), then
    /// signals `DISPLAY_SIZE_CHANGED` (`0x492f57`/`0x492f5c`, event `0x1ff`), cache hit or not.
    /// `display` is the window in logical px, the host's unit for the reference's device size.
    /// `true` when a scale reached `UIParent`.
    pub fn apply_ui_scale(
        &mut self,
        display: (u32, u32),
        auto_cache: &mut Option<(u32, u32)>,
    ) -> bool {
        let (target, signal) = {
            let model = self.model_ref();
            let Some(ui_scale) = model.ui_scale_request else {
                return false;
            };
            let aspect = model.screen_aspect();
            let parent = |v| model.arena.lookup("UIParent").map(|h| (h, v));
            if ui_scale >= UI_SCALE_FLOOR {
                (parent(manual_ui_scale(ui_scale)), false)
            } else {
                let (w, h) = display;
                if w > 0 && h > 0 && *auto_cache == Some(display) {
                    (None, true)
                } else {
                    *auto_cache = Some(display);
                    // The aspect is the cells' (`0x4901e0`), never the display's own `w/h`.
                    (parent(floored_ui_scale(auto_ui_scale(h, aspect))), true)
                }
            }
        };
        let scaled = target.is_some_and(|(ui_parent, v)| {
            self.model_mut().set_frame_scale(ui_parent, v, true);
            true
        });
        if signal {
            self.fire_event("DISPLAY_SIZE_CHANGED", Vec::new());
        }
        scaled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_ui_scale_keeps_the_dial_value_on_narrow_windows_and_floors_at_the_sink() {
        // Wide, narrow, square and portrait windows all keep the same manually selected value.
        assert_eq!(manual_ui_scale(0.7), 0.7);
        assert_eq!(manual_ui_scale(0.9), 0.9);
        assert_eq!(manual_ui_scale(1.0), 1.0);
        assert_eq!(manual_ui_scale(1.2), 1.2);
        // The SetUiScale floor remains in force.
        assert_eq!(manual_ui_scale(0.5), UI_SCALE_FLOOR);
        assert_eq!(manual_ui_scale(0.63), UI_SCALE_FLOOR);
        assert_eq!(manual_ui_scale(0.65), 0.65);
    }

    #[test]
    fn manual_scale_is_applied_directly_for_a_narrow_split_window() {
        let mut s = UiScript::new().unwrap();
        s.set_screen_size(1920.0, 2160.0);
        s.set_screen_aspect(8.0 / 9.0);
        s.run(r#"CreateFrame("Frame", "UIParent")"#).unwrap();

        let scale = |s: &UiScript| s.eval::<f32>("return UIParent:GetScale()").unwrap();
        let mut cache = None;

        assert!(s.seat_ui_scale(0.9, (1920, 2160), &mut cache));
        assert_eq!(scale(&s), 0.9, "load must preserve the 90% dial value");

        assert!(s.set_ui_scale(0.7));
        assert_eq!(scale(&s), 0.7, "70% must remain adjustable in split-screen");

        assert!(s.set_ui_scale(1.0));
        assert_eq!(scale(&s), 1.0, "100% must remain adjustable in split-screen");
    }

    #[test]
    fn the_scale_lands_on_uiparent_alone_and_only_on_a_change() {
        let mut s = UiScript::new().unwrap();
        s.set_screen_size(1024.0, 768.0);
        assert!(!s.set_ui_scale(0.9), "no UIParent yet: nothing to scale");
        s.run(r#"CreateFrame("Frame", "UIParent") CreateFrame("Frame", "Loose")"#)
            .unwrap();
        assert!(!s.set_ui_scale(0.9), "the same value is no change");
        assert!(
            s.apply_ui_scale((1024, 768), &mut None),
            "the load edge applies what was set"
        );
        assert_eq!(s.eval::<f32>("return UIParent:GetScale()").unwrap(), 0.9);
        assert_eq!(s.eval::<f32>("return Loose:GetScale()").unwrap(), 1.0);
        // An addon's own scale holds until the dial moves.
        s.run("UIParent:SetScale(0.7)").unwrap();
        assert!(!s.set_ui_scale(0.9));
        assert_eq!(s.eval::<f32>("return UIParent:GetScale()").unwrap(), 0.7);
        assert!(s.set_ui_scale(0.8));
        assert_eq!(s.eval::<f32>("return UIParent:GetScale()").unwrap(), 0.8);
    }

    /// The automatic scale (`0x492f70`): 1.0 up to 768 tall, `768/h` above it, never under 0.9.
    #[test]
    fn the_auto_scale_follows_the_height_and_stops_at_nine_tenths() {
        assert_eq!(auto_ui_scale(768, 4.0 / 3.0), 1.0);
        assert_eq!(auto_ui_scale(800, 1.6), 0.96);
        assert_eq!(auto_ui_scale(1080, 16.0 / 9.0), 0.9);
        // A tall narrow display: capped, then raised to 0.9.
        assert_eq!(auto_ui_scale(700, 600.0 / 700.0), 0.9);
        // 4/3 itself is never capped.
        assert_eq!(auto_ui_scale(700, 4.0 / 3.0), 1.0);
    }

    /// The display handler (`0x492e90`) never floors: a dial capped below 0.64 falls to the
    /// automatic scale, which skips a display it already ran for; the CVar callback (`0x490770`)
    /// floors instead.
    #[test]
    fn below_the_floor_the_display_handler_falls_to_the_auto_scale() {
        let mut s = UiScript::new().unwrap();
        s.set_screen_size(1024.0, 768.0);
        s.run(
            r#"CreateFrame("Frame", "UIParent")
            sizes = 0
            local f = CreateFrame("Frame", "SizeWatcher")
            f:RegisterEvent("DISPLAY_SIZE_CHANGED")
            f:SetScript("OnEvent", function() sizes = sizes + 1 end)"#,
        )
        .unwrap();
        let scale = |s: &UiScript| s.eval::<f32>("return UIParent:GetScale()").unwrap();
        let size_events = |s: &UiScript| s.eval::<i64>("return sizes").unwrap();
        let mut cache = None;
        assert!(s.seat_ui_scale(0.5, (1280, 800), &mut cache));
        assert_eq!(scale(&s), 0.96, "the load took the auto scale for 1280×800");
        assert_eq!(cache, Some((1280, 800)));
        assert_eq!(size_events(&s), 1);
        // The same display again: the auto scale does nothing, but the event still fires.
        s.run("UIParent:SetScale(0.7)").unwrap();
        assert!(!s.apply_ui_scale((1280, 800), &mut cache));
        assert_eq!(scale(&s), 0.7);
        assert_eq!(size_events(&s), 2, "the load's and the cache hit's");
        // A new display runs it.
        assert!(s.apply_ui_scale((1920, 1080), &mut cache));
        assert_eq!(scale(&s), 0.9);
        // The CVar callback floors.
        assert!(s.set_ui_scale(0.55));
        assert_eq!(scale(&s), UI_SCALE_FLOOR);
        assert_eq!(size_events(&s), 3, "the new display's");
        // At the floor or above, the display handler hands the value to the sink and signals
        // nothing (`0x492f0a` onward).
        assert!(s.seat_ui_scale(0.8, (1920, 1080), &mut cache));
        assert_eq!(scale(&s), 0.8);
        assert_eq!(size_events(&s), 3);
    }
}
