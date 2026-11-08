//! Reading a keyboard shortcut written as glazewm's configuration writes its bindings.

/// The virtual-key codes of `shortcut`, such as `alt+shift+f`, in the order they are pressed.
///
/// The key names are a subset of the ones glazewm accepts: the modifiers, letters, digits, arrows, and space,
/// tab, enter and escape.
pub fn virtual_keys(shortcut: &str) -> Result<Vec<u16>, String> {
    shortcut
        .split('+')
        .map(|name| {
            virtual_key(&name.to_ascii_lowercase())
                .ok_or_else(|| format!("unknown key `{name}` in shortcut `{shortcut}`"))
        })
        .collect()
}

fn virtual_key(name: &str) -> Option<u16> {
    let code = match name {
        "ctrl" | "control" => 0x11,
        "alt" | "menu" => 0x12,
        "shift" => 0x10,
        "win" | "lwin" => 0x5B,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "space" => 0x20,
        "tab" => 0x09,
        "enter" | "return" => 0x0D,
        "escape" => 0x1B,
        _ => match name.as_bytes() {
            [letter @ b'a'..=b'z'] => u16::from(letter.to_ascii_uppercase()),
            [digit @ b'0'..=b'9'] => u16::from(*digit),
            _ => return None,
        },
    };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_modifiers_and_a_letter_in_order() {
        assert_eq!(virtual_keys("alt+shift+f"), Ok(vec![0x12, 0x10, 0x46]));
    }

    #[test]
    fn reads_names_in_any_case_with_their_aliases() {
        assert_eq!(
            virtual_keys("Control+lwin+Right+1+return"),
            Ok(vec![0x11, 0x5B, 0x27, 0x31, 0x0D])
        );
    }

    #[test]
    fn refuses_a_key_it_does_not_know() {
        assert_eq!(
            virtual_keys("alt+f13"),
            Err("unknown key `f13` in shortcut `alt+f13`".into())
        );
    }
}
