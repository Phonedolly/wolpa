//! Key input translation: macOS NSEvent → Neovim nvim_input() strings.
//!
//! This module maps key events (including modifiers) to the notation
//! Neovim expects via `nvim_input()`. See :help key-notation.

/// Translate a key event to a Neovim input string.
///
/// Designed to be called from the Swift IME handler with key code and modifier flags.
/// Returns None for keys that should be handled by the OS, not Neovim.
pub fn translate_key(
    key: &str,
    ctrl: bool,
    alt: bool,
    meta: bool, // Command on macOS
    shift: bool,
) -> Option<String> {
    // Command key combos are intercepted by OS menu system
    if meta {
        return None;
    }

    let mut mods = String::new();
    if ctrl {
        mods.push_str("C-");
    }
    if alt {
        mods.push_str("M-");
    }
    if shift {
        mods.push_str("S-");
    }

    // Normalize key name
    let normal = match key {
        "\u{1b}" => "Esc".to_string(), // Escape
        "\r" | "\n" => "CR".to_string(),
        "\t" => "Tab".to_string(),
        "\u{7f}" => "BS".to_string(),
        " " => "Space".to_string(),
        _ => key.to_string(),
    };

    Some(format!("<{mods}{normal}>"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_key() {
        assert_eq!(
            translate_key("a", false, false, false, false),
            Some("<a>".into())
        );
    }

    #[test]
    fn test_ctrl_key() {
        assert_eq!(
            translate_key("c", true, false, false, false),
            Some("<C-c>".into())
        );
    }

    #[test]
    fn test_meta_ignored() {
        assert_eq!(translate_key("q", false, false, true, false), None);
    }
}
