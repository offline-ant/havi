use makepad_widgets::*;
use keyboard_types::{Code, Key, Modifiers, NamedKey};

pub fn makepad_key_to_servo(key_code: KeyCode) -> Key {
    match key_code {
        KeyCode::KeyA => Key::Character("a".into()),
        KeyCode::KeyB => Key::Character("b".into()),
        KeyCode::KeyC => Key::Character("c".into()),
        KeyCode::KeyD => Key::Character("d".into()),
        KeyCode::KeyE => Key::Character("e".into()),
        KeyCode::KeyF => Key::Character("f".into()),
        KeyCode::KeyG => Key::Character("g".into()),
        KeyCode::KeyH => Key::Character("h".into()),
        KeyCode::KeyI => Key::Character("i".into()),
        KeyCode::KeyJ => Key::Character("j".into()),
        KeyCode::KeyK => Key::Character("k".into()),
        KeyCode::KeyL => Key::Character("l".into()),
        KeyCode::KeyM => Key::Character("m".into()),
        KeyCode::KeyN => Key::Character("n".into()),
        KeyCode::KeyO => Key::Character("o".into()),
        KeyCode::KeyP => Key::Character("p".into()),
        KeyCode::KeyQ => Key::Character("q".into()),
        KeyCode::KeyR => Key::Character("r".into()),
        KeyCode::KeyS => Key::Character("s".into()),
        KeyCode::KeyT => Key::Character("t".into()),
        KeyCode::KeyU => Key::Character("u".into()),
        KeyCode::KeyV => Key::Character("v".into()),
        KeyCode::KeyW => Key::Character("w".into()),
        KeyCode::KeyX => Key::Character("x".into()),
        KeyCode::KeyY => Key::Character("y".into()),
        KeyCode::KeyZ => Key::Character("z".into()),
        KeyCode::Key0 => Key::Character("0".into()),
        KeyCode::Key1 => Key::Character("1".into()),
        KeyCode::Key2 => Key::Character("2".into()),
        KeyCode::Key3 => Key::Character("3".into()),
        KeyCode::Key4 => Key::Character("4".into()),
        KeyCode::Key5 => Key::Character("5".into()),
        KeyCode::Key6 => Key::Character("6".into()),
        KeyCode::Key7 => Key::Character("7".into()),
        KeyCode::Key8 => Key::Character("8".into()),
        KeyCode::Key9 => Key::Character("9".into()),
        KeyCode::Escape => Key::Named(NamedKey::Escape),
        KeyCode::Tab => Key::Named(NamedKey::Tab),
        KeyCode::Space => Key::Character(" ".into()),
        KeyCode::ReturnKey => Key::Named(NamedKey::Enter),
        KeyCode::Backspace => Key::Named(NamedKey::Backspace),
        KeyCode::Delete => Key::Named(NamedKey::Delete),
        KeyCode::Insert => Key::Named(NamedKey::Insert),
        KeyCode::ArrowUp => Key::Named(NamedKey::ArrowUp),
        KeyCode::ArrowDown => Key::Named(NamedKey::ArrowDown),
        KeyCode::ArrowLeft => Key::Named(NamedKey::ArrowLeft),
        KeyCode::ArrowRight => Key::Named(NamedKey::ArrowRight),
        KeyCode::Home => Key::Named(NamedKey::Home),
        KeyCode::End => Key::Named(NamedKey::End),
        KeyCode::PageUp => Key::Named(NamedKey::PageUp),
        KeyCode::PageDown => Key::Named(NamedKey::PageDown),
        KeyCode::Control => Key::Named(NamedKey::Control),
        KeyCode::Alt => Key::Named(NamedKey::Alt),
        KeyCode::Shift => Key::Named(NamedKey::Shift),
        KeyCode::Logo => Key::Named(NamedKey::Meta),
        KeyCode::F1 => Key::Named(NamedKey::F1),
        KeyCode::F2 => Key::Named(NamedKey::F2),
        KeyCode::F3 => Key::Named(NamedKey::F3),
        KeyCode::F4 => Key::Named(NamedKey::F4),
        KeyCode::F5 => Key::Named(NamedKey::F5),
        KeyCode::F6 => Key::Named(NamedKey::F6),
        KeyCode::F7 => Key::Named(NamedKey::F7),
        KeyCode::F8 => Key::Named(NamedKey::F8),
        KeyCode::F9 => Key::Named(NamedKey::F9),
        KeyCode::F10 => Key::Named(NamedKey::F10),
        KeyCode::F11 => Key::Named(NamedKey::F11),
        KeyCode::F12 => Key::Named(NamedKey::F12),
        _ => Key::Named(NamedKey::Unidentified),
    }
}

pub fn makepad_key_to_code(key_code: KeyCode) -> Code {
    match key_code {
        KeyCode::KeyA => Code::KeyA,
        KeyCode::KeyB => Code::KeyB,
        KeyCode::KeyC => Code::KeyC,
        KeyCode::KeyD => Code::KeyD,
        KeyCode::KeyE => Code::KeyE,
        KeyCode::KeyF => Code::KeyF,
        KeyCode::KeyG => Code::KeyG,
        KeyCode::KeyH => Code::KeyH,
        KeyCode::KeyI => Code::KeyI,
        KeyCode::KeyJ => Code::KeyJ,
        KeyCode::KeyK => Code::KeyK,
        KeyCode::KeyL => Code::KeyL,
        KeyCode::KeyM => Code::KeyM,
        KeyCode::KeyN => Code::KeyN,
        KeyCode::KeyO => Code::KeyO,
        KeyCode::KeyP => Code::KeyP,
        KeyCode::KeyQ => Code::KeyQ,
        KeyCode::KeyR => Code::KeyR,
        KeyCode::KeyS => Code::KeyS,
        KeyCode::KeyT => Code::KeyT,
        KeyCode::KeyU => Code::KeyU,
        KeyCode::KeyV => Code::KeyV,
        KeyCode::KeyW => Code::KeyW,
        KeyCode::KeyX => Code::KeyX,
        KeyCode::KeyY => Code::KeyY,
        KeyCode::KeyZ => Code::KeyZ,
        KeyCode::Key0 => Code::Digit0,
        KeyCode::Key1 => Code::Digit1,
        KeyCode::Key2 => Code::Digit2,
        KeyCode::Key3 => Code::Digit3,
        KeyCode::Key4 => Code::Digit4,
        KeyCode::Key5 => Code::Digit5,
        KeyCode::Key6 => Code::Digit6,
        KeyCode::Key7 => Code::Digit7,
        KeyCode::Key8 => Code::Digit8,
        KeyCode::Key9 => Code::Digit9,
        KeyCode::Escape => Code::Escape,
        KeyCode::Tab => Code::Tab,
        KeyCode::Space => Code::Space,
        KeyCode::ReturnKey => Code::Enter,
        KeyCode::Backspace => Code::Backspace,
        KeyCode::Delete => Code::Delete,
        KeyCode::Insert => Code::Insert,
        KeyCode::ArrowUp => Code::ArrowUp,
        KeyCode::ArrowDown => Code::ArrowDown,
        KeyCode::ArrowLeft => Code::ArrowLeft,
        KeyCode::ArrowRight => Code::ArrowRight,
        KeyCode::Home => Code::Home,
        KeyCode::End => Code::End,
        KeyCode::PageUp => Code::PageUp,
        KeyCode::PageDown => Code::PageDown,
        KeyCode::Control => Code::ControlLeft,
        KeyCode::Alt => Code::AltLeft,
        KeyCode::Shift => Code::ShiftLeft,
        KeyCode::Logo => Code::MetaLeft,
        KeyCode::F1 => Code::F1,
        KeyCode::F2 => Code::F2,
        KeyCode::F3 => Code::F3,
        KeyCode::F4 => Code::F4,
        KeyCode::F5 => Code::F5,
        KeyCode::F6 => Code::F6,
        KeyCode::F7 => Code::F7,
        KeyCode::F8 => Code::F8,
        KeyCode::F9 => Code::F9,
        KeyCode::F10 => Code::F10,
        KeyCode::F11 => Code::F11,
        KeyCode::F12 => Code::F12,
        _ => Code::Unidentified,
    }
}

pub fn makepad_modifiers_to_servo(modifiers: KeyModifiers) -> Modifiers {
    let mut result = Modifiers::empty();
    if modifiers.shift {
        result |= Modifiers::SHIFT;
    }
    if modifiers.control {
        result |= Modifiers::CONTROL;
    }
    if modifiers.alt {
        result |= Modifiers::ALT;
    }
    if modifiers.logo {
        result |= Modifiers::META;
    }
    result
}

pub fn translate_key_event(ke: &KeyEvent, is_down: bool) -> Option<servo::InputEvent> {
    let key = makepad_key_to_servo(ke.key_code);
    let code = makepad_key_to_code(ke.key_code);
    let modifiers = makepad_modifiers_to_servo(ke.modifiers);
    let state = if is_down {
        keyboard_types::KeyState::Down
    } else {
        keyboard_types::KeyState::Up
    };

    // Strip Key::Character from keyboard events.  Makepad fires both KeyDown
    // and TextInput for every character key.  If we send Key::Character here,
    // Servo inserts text from the keypress event AND from the IME composition
    // (TextInput path), causing double input.  By using Unidentified, Servo
    // still dispatches keydown/keyup DOM events (for shortcuts, tab, etc.)
    // but does not treat the keydown as a text-inserting keypress.
    let key = match key {
        Key::Character(_) => Key::Named(NamedKey::Unidentified),
        other => other,
    };

    Some(servo::InputEvent::Keyboard(
        servo::KeyboardEvent::new(keyboard_types::KeyboardEvent {
            state,
            key,
            code,
            location: keyboard_types::Location::Standard,
            modifiers,
            repeat: ke.is_repeat,
            is_composing: false,
        }),
    ))
}
