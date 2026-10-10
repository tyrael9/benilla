//! The player-UI input pass: [`feed_ui_input`] feeds mouse and keyboard events into the UI engine
//! once [`super::extract::tick_script`] has resolved the frame's rects.

use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use benilla_ui::script::UiScript;

use super::{CursorPayloadHeld, PlayerUiClickConsumed, PlayerUiHover, UiKeyboardCapture};
use crate::bindings::WheelNotches;
use crate::textinput::{self, keymap, HostClipboard};

#[cfg(any(target_os = "ios", test))]
fn touch_stick_contains(
    x: f32,
    y: f32,
    center_x: f32,
    center_y: f32,
    radius: f32,
) -> bool {
    let dx = x - center_x;
    let dy = y - center_y;
    dx * dx + dy * dy <= radius * radius
}

#[cfg(any(target_os = "ios", test))]
fn touch_stick_axes(
    x: f32,
    y: f32,
    center_x: f32,
    center_y: f32,
    travel: f32,
) -> Vec2 {
    // The input's full-scale travel must match the visual thumb center's allowed displacement.
    Vec2::new((x - center_x) / travel, (y - center_y) / travel)
        .clamp_length_max(1.0)
}

#[cfg(any(target_os = "ios", test))]
fn touch_look_started_drag(start: Vec2, current: Vec2) -> bool {
    // Ignore touch jitter so world taps still select, while real drags rotate the camera.
    start.distance_squared(current) >= 64.0
}

/// The pointer-side state [`feed_ui_input`] reads and writes. The world pick reads last frame's
/// hover and occlusion ray, since the target chain runs after this pass; one frame of staleness is
/// within the pick's tolerance.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct PointerFeed<'w> {
    hover: ResMut<'w, PlayerUiHover>,
    click_consumed: ResMut<'w, PlayerUiClickConsumed>,
    hovered: Res<'w, crate::target::Hovered>,
    hovered_object: Res<'w, crate::target::HoveredObject>,
    occlusion: Res<'w, crate::target::PickOcclusion>,
    payload_held: ResMut<'w, CursorPayloadHeld>,
    /// A headless probe drives the pointer ([`super::SyntheticPointer`]), not the real cursor.
    synthetic: Res<'w, super::SyntheticPointer>,
    /// A capture owns the pointer ([`super::CapturePointerPinned`]): no OS cursor in the shot.
    capture_pinned: Res<'w, super::CapturePointerPinned>,
}

impl PointerFeed<'_> {
    /// The reference's click-time pick: a hovered unit or GameObject is `Object`, else a finite
    /// occlusion-ray hit (terrain, WMO, doodad) is `Terrain`, else `Nothing` (sky).
    fn world_pick(&self) -> benilla_ui::script::WorldPick {
        use benilla_ui::script::WorldPick;
        if self.hovered.target.is_some() || self.hovered_object.target.is_some() {
            WorldPick::Object
        } else if self.occlusion.distance.is_finite() {
            WorldPick::Terrain
        } else {
            WorldPick::Nothing
        }
    }
}

/// The buttons the pointer feed hands the UI, by the names its handlers read (`arg1` of `OnClick`,
/// `OnMouseDown` and `OnMouseUp`, and the ones `RegisterForClicks` takes,
/// `Blizzard_BindingUI.xml:10`); buttons 4 and 5 on the physical buttons the binding chords call
/// BUTTON4 and BUTTON5 (`bindings::chord`).
const UI_MOUSE_BUTTONS: [(MouseButton, &str); 5] = [
    (MouseButton::Left, "LeftButton"),
    (MouseButton::Right, "RightButton"),
    (MouseButton::Middle, "MiddleButton"),
    (MouseButton::Forward, "Button4"),
    (MouseButton::Back, "Button5"),
];

/// One `OnMouseWheel` call per whole notch, `arg1 = ±1`, the fraction carried in `notches`: a
/// trackpad gesture arrives as a `Pixel` trickle, and the stock handlers act on the sign alone
/// (`ScrollFrameTemplate_OnMouseWheel` scrolls half a pane per call, `UIPanelTemplates.lua:150`).
fn feed_wheel(
    script: &mut UiScript,
    notches: &mut WheelNotches,
    x: f32,
    y: f32,
    scroll: &AccumulatedMouseScroll,
) {
    let whole = notches.feed(crate::bindings::wheel_lines(scroll.unit, scroll.delta.y));
    let step = whole.signum() as f32;
    for _ in 0..whole.unsigned_abs() {
        script.mouse_wheel(x, y, step);
    }
}

/// Feeds cursor, buttons, wheel and keys into the UI engine, publishing [`PlayerUiHover`] (the
/// pointer arbiter yields to the UI) and [`UiKeyboardCapture`] (a key a box or frame ate never
/// fires its binding). Runs in the `UiInput` set, before `WorldStage::Input` and every key reader.
pub(super) fn feed_ui_input(
    script: Option<NonSendMut<UiScript>>,
    // Carries the `wl_display` the Wayland clipboard backend needs, once winit has a surface.
    mut window: Query<(&mut Window, Option<&bevy::window::RawHandleWrapper>), With<PrimaryWindow>>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    // The wheel travel and its carried notch fraction, one param for clippy's argument ceiling.
    (scroll, mut notches): (Res<AccumulatedMouseScroll>, ResMut<WheelNotches>),
    mut pointer: PointerFeed,
    // One param for clippy's argument ceiling, with the pasteboard the clipboard chords use.
    mut kbd: (
        MessageReader<KeyboardInput>,
        Res<ButtonInput<KeyCode>>,
        ResMut<UiKeyboardCapture>,
        NonSendMut<HostClipboard>,
    ),
    // The characters the active layout makes, which name a key for a keyboard frame.
    layout: Res<crate::bindings::LayoutNames>,
    // The uiScale dial folded into the seam scale.
    ui_scale: Res<super::UiScaleCvar>,
    time: Res<Time>,
    mut gamepad: ResMut<crate::gamepad::GamepadState>,
    mut virtual_cursor: ResMut<crate::cursor::VirtualCursor>,
    mut virtual_mouse_buttons: ResMut<crate::cursor::VirtualMouseButtons>,
    // iOS has touch input rather than a conventional OS mouse pointer.
    mut touches: MessageReader<bevy::input::touch::TouchInput>,
    // One touch is routed through WoW's single-pointer UI model; a separate finger can own
    // the virtual movement stick without moving that UI pointer.
    mut touch_ui_id: Local<Option<u64>>,
    mut touch_move_id: Local<Option<u64>>,
    // (touch id, start position, previous position, crossed-drag-threshold)
    mut touch_look: Local<Option<(u64, Vec2, Vec2, bool)>>,
) {
    const GAMEPAD_MOUSE_SPEED: f32 = 700.0;

    let (keyboard, keys, capture, clipboard) = (&mut kbd.0, &kbd.1, &mut kbd.2, &mut kbd.3);
    let world_pick = pointer.world_pick();
    // The OS pointer is not ours while a probe drives a gesture through the real pointer path or a
    // capture pins it: skip the mouse half whole, else-arm included, whose `pointer_left_window`
    // would disarm the probe's gesture between its press and release.
    let synthetic = pointer.synthetic.0 || pointer.capture_pinned.0;
    let (hover, click_consumed, payload_held) = (
        &mut pointer.hover,
        &mut pointer.click_consumed,
        &mut pointer.payload_held,
    );
    click_consumed.0 = false;
    let Some(mut script) = script else {
        capture.typing = false;
        capture.arrows_fall_through = false;
        capture.consumed.clear();
        payload_held.0 = false;
        return;
    };
    let Ok((mut window, raw_handle)) = window.single_mut() else {
        capture.typing = false;
        capture.arrows_fall_through = false;
        capture.consumed.clear();
        payload_held.0 = false;
        return;
    };
    let wl_display = textinput::wayland_display(raw_handle);
    // The pick routes the world drop: an object keeps the payload, terrain or sky drops it.
    script.set_world_pick(world_pick);
    // ── Modifiers ── before the mouse feed, so a click's `IsShiftKeyDown` sees the click's state.
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    script.set_modifiers(shift, ctrl, alt);
    // ── Mouse ──────────────────────────────────────────────────────────────────────────────────
    //
    // LT turns the right stick into a virtual mouse.
    //
    // virtual_mouse_x/y use the same y-up coordinate system as UiScript::mouse_move().
    // The real OS cursor uses y-down window coordinates, so we convert when warping it.

    let gamepad_mouse = gamepad.zl > 0.5;

    // iOS touches have independent ownership: one finger may drive movement while another
    // taps a WoW UI control. Only the UI-owned finger is translated into the client's single
    // mouse pointer; the movement finger never relocates that pointer or synthesizes a click.
    #[cfg(target_os = "ios")]
    let mut ios_touch_event_this_frame = false;
    #[cfg(target_os = "ios")]
    {
        gamepad.touch_look_delta = Vec2::ZERO;
        gamepad.touch_look_active = touch_look.as_ref().map(|state| state.3).unwrap_or(false);

        use bevy::input::touch::TouchPhase;
        let mut touch_events = touches.read().peekable();
        if touch_events.peek().is_some() {
            // eval::<T> reads one return value, so retrieve the saved stick config as scalar values.
            let stick_x = script.eval::<f32>(
                "return Gamepad_GetTouchStickValue('X')"
            ).unwrap_or(132.0);
            let stick_y = script.eval::<f32>(
                "return Gamepad_GetTouchStickValue('Y')"
            ).unwrap_or(136.0);
            let stick_radius = script.eval::<f32>(
                "return Gamepad_GetTouchStickValue('Radius')"
            ).unwrap_or(96.0);
            let stick_travel = script.eval::<f32>(
                "return Gamepad_GetTouchStickValue('Travel')"
            ).unwrap_or(63.0);
            let stick_enabled = script.eval::<bool>(
                "return Gamepad_IsTouchStickEnabled()"
            ).unwrap_or(true);
            let stick_unlocked = script.eval::<bool>(
                "return Gamepad_IsTouchStickUnlocked()"
            ).unwrap_or(false);

            for touch in touch_events {
            let raw_position = touch.position;
            let s = super::seam_scale(window.height(), ui_scale.0);
            let (x, y) = (touch.position.x / s, (window.height() - touch.position.y) / s);
            let owns_look = touch_look.as_ref().map(|state| state.0) == Some(touch.id);

            // Geometry and drag mode are published by BenillaMobile.
            // A UI frame (including the stick while unlocked) takes priority over movement.
            let in_move_stick = stick_enabled
                && !stick_unlocked
                && touch_stick_contains(
                    x,
                    y,
                    stick_x,
                    stick_y,
                    stick_radius,
                );
            // The reserved movement zone must win over background UI frames (for example the
            // chat panel). Otherwise hit-testing consumes the finger before it reaches the stick.
            let claim_move = matches!(touch.phase, TouchPhase::Started)
                && in_move_stick
                && touch_move_id.is_none();
            let can_route_ui = !gamepad_mouse && touch_ui_id.is_none();
            // Do not move the single WoW UI pointer once a finger has been captured for camera look.
            let hit_frame = if can_route_ui && !claim_move && !owns_look {
                script.mouse_move(x, y)
            } else {
                None
            };
            let hit = hit_frame.filter(|id| !script.is_world_frame(*id));

            match touch.phase {
                TouchPhase::Started => {
                    ios_touch_event_this_frame = true;

                    if claim_move {
                        *touch_move_id = Some(touch.id);
                        gamepad.touch_move_active = true;
                        gamepad.touch_move_stick =
                            touch_stick_axes(x, y, stick_x, stick_y, stick_travel);
                        hover.0 = None;
                    } else if can_route_ui && hit.is_none() {
                        // Defer a world click until release: if this becomes a drag, it is camera
                        // input instead of an accidental target click.
                        if touch_look.is_none() {
                            *touch_look = Some((touch.id, raw_position, raw_position, false));
                        }
                        hover.0 = None;
                    } else if can_route_ui {
                        *touch_ui_id = Some(touch.id);
                        hover.0 = hit;
                        script.mouse_button(x, y, "LeftButton", true);
                    }
                }
                TouchPhase::Moved if *touch_move_id == Some(touch.id) => {
                    ios_touch_event_this_frame = true;
                    gamepad.touch_move_stick =
                        touch_stick_axes(x, y, stick_x, stick_y, stick_travel);
                }
                TouchPhase::Moved
                    if touch_look.as_ref().map(|state| state.0) == Some(touch.id) =>
                {
                    ios_touch_event_this_frame = true;
                    let (_, start, previous, was_dragging) = *touch_look.as_ref().unwrap();
                    let dragging = was_dragging || touch_look_started_drag(start, raw_position);
                    if dragging {
                        gamepad.touch_look_delta += if was_dragging {
                            raw_position - previous
                        } else {
                            raw_position - start
                        };
                        gamepad.touch_look_active = true;
                    }
                    *touch_look = Some((touch.id, start, raw_position, dragging));
                }
                TouchPhase::Moved if *touch_ui_id == Some(touch.id) => {
                    ios_touch_event_this_frame = true;
                    hover.0 = script.mouse_move(x, y)
                        .filter(|id| !script.is_world_frame(*id));
                }
                TouchPhase::Ended | TouchPhase::Canceled
                    if *touch_move_id == Some(touch.id) =>
                {
                    ios_touch_event_this_frame = true;
                    gamepad.touch_move_stick = Vec2::ZERO;
                    gamepad.touch_move_active = false;
                    *touch_move_id = None;
                }
                TouchPhase::Ended | TouchPhase::Canceled
                    if *touch_ui_id == Some(touch.id) =>
                {
                    ios_touch_event_this_frame = true;
                    hover.0 = script.mouse_move(x, y)
                        .filter(|id| !script.is_world_frame(*id));
                    script.mouse_button(x, y, "LeftButton", false);
                    *touch_ui_id = None;
                }
                TouchPhase::Ended
                    if touch_look.as_ref().map(|state| state.0) == Some(touch.id) =>
                {
                    ios_touch_event_this_frame = true;
                    let (_, _, _, dragging) = *touch_look.as_ref().unwrap();
                    if !dragging {
                        // It was a tap, not a drag: preserve the existing world click behavior.
                        hover.0 = script.mouse_move(x, y)
                            .filter(|id| !script.is_world_frame(*id));
                        script.mouse_button(x, y, "LeftButton", true);
                        script.mouse_button(x, y, "LeftButton", false);
                    }
                    *touch_look = None;
                    gamepad.touch_look_active = false;
                }
                TouchPhase::Canceled
                    if touch_look.as_ref().map(|state| state.0) == Some(touch.id) =>
                {
                    ios_touch_event_this_frame = true;
                    *touch_look = None;
                    gamepad.touch_look_active = false;
                }
                _ => {}
            }
            }
        }
    }

    // ── Virtual mouse → Bevy mouse buttons ─────────────────────────────────────
    //
    // While LT is held, A/B are the mouse buttons for this Benilla instance.
    // Inject them into Bevy's normal ButtonInput so the existing world-input
    // pipeline sees exactly the same mouse events as a physical mouse.

    if gamepad_mouse {
        let want_left = gamepad.a;
        let want_right = gamepad.b;

        if want_left && !virtual_mouse_buttons.left {
            buttons.press(MouseButton::Left);
        } else if !want_left && virtual_mouse_buttons.left {
            buttons.release(MouseButton::Left);
        }

        if want_right && !virtual_mouse_buttons.right {
            buttons.press(MouseButton::Right);
        } else if !want_right && virtual_mouse_buttons.right {
            buttons.release(MouseButton::Right);
        }

        virtual_mouse_buttons.left = want_left;
        virtual_mouse_buttons.right = want_right;
    } else {
        // LT released: release any buttons owned by the virtual mouse.
        if virtual_mouse_buttons.left {
            buttons.release(MouseButton::Left);
        }

        if virtual_mouse_buttons.right {
            buttons.release(MouseButton::Right);
        }

        virtual_mouse_buttons.left = false;
        virtual_mouse_buttons.right = false;
    }

    if gamepad_mouse {
        let s = super::seam_scale(window.height(), ui_scale.0);

        if !gamepad.virtual_mouse_initialized {
            // Each Benilla instance owns its own virtual cursor.
            // Do NOT read or move the OS cursor here.
            gamepad.virtual_mouse_x = window.width() / (2.0 * s);
            gamepad.virtual_mouse_y = window.height() / (2.0 * s);

            gamepad.virtual_mouse_initialized = true;
        }

        // Right stick → virtual mouse.
        //
        // look_stick is still the normal gamepad stick value.
        // Camera itself is suppressed in look_axes() while LT is held.
        let dt = time.delta_secs();

        let stick = if gamepad.look_stick.length() < 0.15 {
            Vec2::ZERO
        } else {
            gamepad.look_stick
        };

        gamepad.virtual_mouse_x +=
            stick.x * GAMEPAD_MOUSE_SPEED * dt;

        gamepad.virtual_mouse_y -=
            stick.y * GAMEPAD_MOUSE_SPEED * dt;

        let max_x = window.width() / s;
        let max_y = window.height() / s;

        gamepad.virtual_mouse_x =
            gamepad.virtual_mouse_x.clamp(0.0, max_x);

        gamepad.virtual_mouse_y =
            gamepad.virtual_mouse_y.clamp(0.0, max_y);

        let x = gamepad.virtual_mouse_x;
        let y = gamepad.virtual_mouse_y;
        virtual_cursor.position = Some(Vec2::new(x, y));
        virtual_cursor.active = true;

        // Feed the per-instance virtual cursor directly into the WoW UI.
        //
        // IMPORTANT:
        // Do NOT warp the OS cursor. Each Benilla instance owns
        // its own virtual cursor independently.
        hover.0 = script
            .mouse_move(x, y)
            .filter(|id| !script.is_world_frame(*id));

        // A → virtual left mouse button.
        if gamepad.a && !gamepad.prev_a {
            script.mouse_button(x, y, "LeftButton", true);
            gamepad.virtual_mouse_left_down = true;
        }

        if !gamepad.a && gamepad.prev_a {
            script.mouse_button(x, y, "LeftButton", false);
            gamepad.virtual_mouse_left_down = false;
        }

        // B → virtual right mouse button.
        if gamepad.b && !gamepad.prev_b {
            script.mouse_button(x, y, "RightButton", true);
            gamepad.virtual_mouse_right_down = true;
        }

        if !gamepad.b && gamepad.prev_b {
            script.mouse_button(x, y, "RightButton", false);
            gamepad.virtual_mouse_right_down = false;
        }
    } else {
        // LT released.
        //
        // Always release any virtual mouse button that might still be held.
        if gamepad.virtual_mouse_left_down {
            script.mouse_button(
                gamepad.virtual_mouse_x,
                gamepad.virtual_mouse_y,
                "LeftButton",
                false,
            );
            gamepad.virtual_mouse_left_down = false;
        }

        if gamepad.virtual_mouse_right_down {
            script.mouse_button(
                gamepad.virtual_mouse_x,
                gamepad.virtual_mouse_y,
                "RightButton",
                false,
            );
            gamepad.virtual_mouse_right_down = false;
        }

        virtual_cursor.position = None;
        virtual_cursor.active = false;

        // Keep virtual_mouse_x/y and initialized state.
        //
        // The physical mouse path below remains completely unchanged.

        // Normal physical mouse path.
        if let Some(cursor) = window
            .cursor_position()
            .or_else(crate::target::hover_probe_point)
            .filter(|_| !synthetic)
            .filter(|_| {
                #[cfg(target_os = "ios")]
                {
                    !ios_touch_event_this_frame
                        && touch_ui_id.is_none()
                        && touch_move_id.is_none()
                        && touch_look.is_none()
                }
                #[cfg(not(target_os = "ios"))]
                { true }
            })
        {
            let s = super::seam_scale(window.height(), ui_scale.0);
            let (x, y) = (
                cursor.x / s,
                (window.height() - cursor.y) / s,
            );

            static HIT_COST: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            let metering =
                *HIT_COST.get_or_init(|| std::env::var("WOW_HIT_COST").as_deref() == Ok("1"));
            let t0 = metering.then(std::time::Instant::now);

            hover.0 = script
                .mouse_move(x, y)
                .filter(|id| !script.is_world_frame(*id));

            if let Some(t0) = t0 {
                use std::sync::atomic::{AtomicU64, Ordering};

                static ACC_US: AtomicU64 = AtomicU64::new(0);
                static CALLS: AtomicU64 = AtomicU64::new(0);
                static LAST: AtomicU64 = AtomicU64::new(0);

                ACC_US.fetch_add(
                    t0.elapsed().as_micros() as u64,
                    Ordering::Relaxed,
                );

                let calls = CALLS.fetch_add(1, Ordering::Relaxed) + 1;

                let now_s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                if LAST.swap(now_s, Ordering::Relaxed) != now_s && calls > 1 {
                    let us = ACC_US.swap(0, Ordering::Relaxed);
                    let n = CALLS.swap(0, Ordering::Relaxed);

                    eprintln!(
                        "[hit-cost] mouse_move_us/frame={:.1} frames={n}",
                        us as f64 / n.max(1) as f64
                    );
                }
            }

            for (btn, name) in UI_MOUSE_BUTTONS {
                if buttons.just_pressed(btn) {
                    if btn == MouseButton::Left && hover.0.is_none() {
                        use benilla_ui::script::WorldPick;

                        let would_drop =
                            script.cursor_payload().is_some()
                                && world_pick != WorldPick::Object;

                        if would_drop {
                            click_consumed.0 = true;
                        }
                    }

                    script.mouse_button(x, y, name, true);
                }

                if buttons.just_released(btn) {
                    script.mouse_button(x, y, name, false);
                }
            }

            feed_wheel(
                &mut script,
                &mut notches,
                x,
                y,
                &scroll,
            );
        } else if !synthetic && {
            #[cfg(target_os = "ios")]
            { touch_ui_id.is_none() && touch_move_id.is_none() && touch_look.is_none() }
            #[cfg(not(target_os = "ios"))]
            { true }
        } {
            if hover.0.take().is_some() {
                script.mouse_move(f32::MIN, f32::MIN);
            }

            script.pointer_left_window();
        }
    }

    // ── Keyboard capture gate ── read after the mouse feed (a click may have just focused a box)
    // and before the keys (an Escape that clears focus still counts as captured this frame).
    capture.typing = script.has_keyboard_focus();
    // Match the login screen's iOS bridge: winit's iOS view presents UIKit's software keyboard
    // only while IME is enabled. World EditBoxes (notably ChatFrameEditBox) never went through
    // login_input, so without this the same focus worked on desktop but could not accept typing.
    #[cfg(target_os = "ios")]
    {
        window.ime_enabled = capture.typing;
    }
    capture.consumed.clear();
    // ── The alt-arrow exemption ────────────────────────────────────────────────────────────────
    // An EditBox in alt-arrow mode (`ignoreArrows`, set on the chat box at `ChatFrame.xml:21`)
    // passes the arrows on unless ALT is held: its handler returns 0 at `0x77b1c4` and the
    // binding runs at `CGWorldFrame`; a focused box swallows any other key (`0x77b35e` returns 1).
    capture.arrows_fall_through = !alt && script.editbox_alt_arrow_mode();

    // ── Keyboard → VM ── box-event keys go to `key_input` by name, editing keys through the
    // per-OS chord table as an `EditAction` or a clipboard operation (the macOS NSPasteboard
    // needs this main-thread system), the rest to `char_input`. Repeats arrive as `Pressed`.
    let sup = keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight);
    let mods = keymap::Mods {
        shift,
        ctrl,
        alt,
        sup,
    };
    let mac = cfg!(target_os = "macos");
    for ev in keyboard.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }

        // Some iOS software-keyboard layouts deliver the Return action as a newline text
        // event instead of a physical Enter key code. Route that through the same FrameXML
        // key path as Enter so ChatFrameEditBox.OnEnterPressed can send the message, and never
        // insert the newline into the single-line edit box.
        #[cfg(target_os = "ios")]
        if !matches!(ev.key_code, KeyCode::Enter | KeyCode::NumpadEnter)
            && ev.text.as_deref().is_some_and(|text| matches!(text, "\n" | "\r" | "\r\n"))
        {
            script.key_input("ENTER");
            capture.consumed.push(ev.key_code);
            continue;
        }

        let named = match ev.key_code {
            KeyCode::Enter | KeyCode::NumpadEnter => Some("ENTER"),
            KeyCode::Escape => Some("ESCAPE"),
            KeyCode::Tab => Some("TAB"),
            _ => None,
        };
        // What the layout names the key: the clipboard chords' letters, and a keyboard frame's
        // `arg1` below.
        let token = crate::bindings::chord::key_token(ev.key_code, &layout);
        // Dispatched unconditionally: unfocused, they fall through to the camera and turn keys.
        let chord = keymap::chord(ev.key_code, token, mods, mac);
        // A keyboard frame gets these by name before their chord runs (a dialog needs BACKSPACE);
        // `frame_key_input` declines at a focused box, so no frame steals its editing keys. `true`
        // suppresses the chord and the key's binding (the reference's existence gate, `0x76b7d0`).
        let frame_named = match ev.key_code {
            KeyCode::Backspace => Some("BACKSPACE"),
            KeyCode::Delete => Some("DELETE"),
            KeyCode::ArrowLeft => Some("LEFT"),
            KeyCode::ArrowRight => Some("RIGHT"),
            KeyCode::ArrowUp => Some("UP"),
            KeyCode::ArrowDown => Some("DOWN"),
            KeyCode::Home => Some("HOME"),
            KeyCode::End => Some("END"),
            _ => None,
        };
        if let Some(name) = frame_named {
            if script.frame_key_input(name) {
                // Not `typing`: a frame eating one key is not a box taking focus, and treating it
                // as one would stop a held movement key.
                capture.consumed.push(ev.key_code);
                continue;
            }
        }
        // Every other key reaches a keyboard frame by name too: the reference's key-down walk
        // takes its `arg1` from the table the binding chord uses (`0x4b66b0`), `chord::key_token`
        // under the active layout here, so the Key Bindings window stores the name the key's
        // press dispatches by. The gate is existence, not handling: a shown keyboard frame with
        // an `OnKeyDown` swallows the key whatever its script does (`0x76b7d0`, `0x76ba25`).
        // Consumption suppresses only the key's binding: `OnChar` is a separate dispatcher
        // (`0x765df0`), so the stack-split spinner still gets a digit its `OnKeyDown` ate, and it
        // is not a focus change, so it releases nothing held.
        else if named.is_none() {
            if let Some(token) = token {
                if script.frame_key_input(&token.to_string()) {
                    capture.consumed.push(ev.key_code);
                }
            }
        }
        if let Some(name) = named {
            // An unconsumed press is not acted on here: ESCAPE's close ladder, TAB's targeting and
            // ENTER's chat open live in the binding dispatch (`crate::bindings`), which runs after
            // this pass and reads the capture gate.
            if script.key_input(name) {
                capture.consumed.push(ev.key_code);
            }
        } else if let Some(chord) = chord {
            // The gate is on the key, not the action: a gated arrow never reaches the box (the
            // reference's `return 0`), while HOME/END, which also make a `Move`, do.
            let gated_arrow = capture.arrows_fall_through
                && matches!(
                    ev.key_code,
                    KeyCode::ArrowLeft
                        | KeyCode::ArrowRight
                        | KeyCode::ArrowUp
                        | KeyCode::ArrowDown
                );
            match chord {
                keymap::Chord::Edit(action) if !gated_arrow => {
                    script.editbox_action(action);
                }
                // Declined: the key falls through to its binding, which `bindings.rs` allows
                // because `arrows_fall_through` exempts exactly these four from the typing gate.
                keymap::Chord::Edit(_) => {}
                // Copy and cut need a selection (`0x77e1d0`), and a password box yields its mask
                // run, never the real text; paste sanitizes and inserts.
                keymap::Chord::Copy => {
                    if let Some(text) = script.editbox_copy() {
                        clipboard.write(wl_display, &text);
                    }
                }
                keymap::Chord::Cut => {
                    if let Some(text) = script.editbox_cut() {
                        clipboard.write(wl_display, &text);
                    }
                }
                keymap::Chord::Paste => {
                    if let Some(text) = clipboard.read(wl_display) {
                        script.paste(&text);
                    }
                }
            }
        } else if !(sup || (ctrl && !alt)) {
            // Command-modified chars never insert (Cmd+L must not type "l"), but Ctrl+Alt passes:
            // that is AltGr, and macOS Option+letter arrives with only `alt`.
            if let Some(text) = &ev.text {
                // Same suppression as the named keys: the number row is a binding, so a digit typed
                // into a keyboard frame (the stack-split spinner) must not also fire an action.
                if script.char_input(text) {
                    capture.consumed.push(ev.key_code);
                }
            }
        }
    }

    // The payload-held mirror, written last so a same-frame pickup or ESC clear shows: the
    // Send-side view the world-click deselect gates on.
    payload_held.0 = script.cursor_payload().is_some();

    for err in script.take_errors() {
        warn!("ui_script(input): {err}");
    }
}

#[cfg(test)]
mod touch_stick_tests {
    use super::*;

    #[test]
    fn world_touch_requires_a_real_drag_to_switch_from_tap_to_camera_look() {
        assert!(!touch_look_started_drag(Vec2::ZERO, Vec2::new(3.0, 3.0)));
        assert!(touch_look_started_drag(Vec2::ZERO, Vec2::new(8.0, 0.0)));
    }

    #[test]
    fn touch_stick_center_and_cardinal_axes_follow_saved_geometry() {
        let (cx, cy, hit_radius, travel) = (132.0, 136.0, 112.0, 71.0);
        assert!(touch_stick_contains(cx, cy, cx, cy, hit_radius));
        assert!(!touch_stick_contains(cx + hit_radius + 1.0, cy, cx, cy, hit_radius));
        assert_eq!(touch_stick_axes(cx, cy, cx, cy, travel), Vec2::ZERO);

        // The displayed nub offset is inverse-mapped to exactly the same finger offset.
        let finger_delta = 40.0;
        let axis = touch_stick_axes(cx + finger_delta, cy, cx, cy, travel);
        assert!((axis.x * travel - finger_delta).abs() < 0.001);

        assert_eq!(touch_stick_axes(cx, cy + travel, cx, cy, travel), Vec2::Y);
        assert_eq!(touch_stick_axes(cx + travel, cy, cx, cy, travel), Vec2::X);
        assert_eq!(touch_stick_axes(cx + hit_radius, cy, cx, cy, travel), Vec2::X);

        let diagonal = touch_stick_axes(cx + travel, cy + travel, cx, cy, travel);
        assert!(diagonal.length() <= 1.00001);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::mouse::MouseScrollUnit;

    fn wheel_pane() -> UiScript {
        let mut s = UiScript::new().unwrap();
        s.set_screen_size(1024.0, 768.0);
        s.run(
            r#"
            local f = CreateFrame("Frame", "WheelProbe")
            f:SetWidth(200) f:SetHeight(200) f:SetPoint("CENTER", nil, "CENTER", 0, 0)
            f:EnableMouseWheel(true)
            f:SetScript("OnMouseWheel", function()
                WheelCalls = (WheelCalls or 0) + 1
                WheelSum = (WheelSum or 0) + arg1
            end)
            f:Show()
            "#,
        )
        .unwrap();
        s.resolve();
        s
    }

    fn calls(s: &UiScript) -> (i64, f64) {
        s.eval::<(Option<i64>, Option<f64>)>("return WheelCalls, WheelSum")
            .map(|(c, v)| (c.unwrap_or(0), v.unwrap_or(0.0)))
            .unwrap()
    }

    fn frame_of(unit: MouseScrollUnit, dy: f32) -> AccumulatedMouseScroll {
        AccumulatedMouseScroll {
            unit,
            delta: Vec2::new(0.0, dy),
        }
    }

    #[test]
    fn a_trackpad_trickle_fires_only_the_notches_it_adds_up_to() {
        let mut s = wheel_pane();
        let mut notches = WheelNotches::default();
        let step = MouseScrollUnit::SCROLL_UNIT_CONVERSION_FACTOR / 10.0;
        for _ in 0..10 {
            feed_wheel(
                &mut s,
                &mut notches,
                512.0,
                384.0,
                &frame_of(MouseScrollUnit::Pixel, step),
            );
        }
        let (n, sum) = calls(&s);
        assert!(
            n <= 1,
            "ten frames adding up to one line fired {n} wheel calls (sum {sum})"
        );
        assert!(sum <= 1.0, "…and they may move at most one notch: {sum}");
    }

    /// Every button 1.12's frames take reaches the UI, under the name its binding chord agrees
    /// with, so the Key Bindings page binds mouse 4 and 5 from a click.
    #[test]
    fn the_ui_takes_all_five_buttons_under_the_chords_names() {
        let names: Vec<&str> = UI_MOUSE_BUTTONS.iter().map(|(_, n)| *n).collect();
        assert_eq!(
            names,
            [
                "LeftButton",
                "RightButton",
                "MiddleButton",
                "Button4",
                "Button5"
            ]
        );
        for (i, (button, _)) in UI_MOUSE_BUTTONS.iter().enumerate() {
            let chord = crate::bindings::chord::Chord::parse(&format!("BUTTON{}", i + 1));
            assert_eq!(
                chord.map(|c| c.key),
                Some(crate::bindings::chord::BindKey::Mouse(*button)),
                "BUTTON{} is {button:?}",
                i + 1
            );
        }
    }

    #[test]
    fn a_line_notch_fires_once() {
        let mut s = wheel_pane();
        let mut notches = WheelNotches::default();
        feed_wheel(
            &mut s,
            &mut notches,
            512.0,
            384.0,
            &frame_of(MouseScrollUnit::Line, 1.0),
        );
        assert_eq!(calls(&s), (1, 1.0));
        feed_wheel(
            &mut s,
            &mut notches,
            512.0,
            384.0,
            &frame_of(MouseScrollUnit::Line, -1.0),
        );
        assert_eq!(calls(&s), (2, 0.0), "and the other way is a notch down");
    }
}
