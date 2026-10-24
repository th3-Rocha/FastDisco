//! Text shaping fixes shared by the model and the views.

/// Inter draws U+002D as wide as a minus sign, so "Boas-vindas" gets gaps
/// around the dash. A dash touching a letter is a hyphen: draw U+2010.
pub fn hyphens(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('-') {
        return text.into();
    }
    let chars: Vec<char> = text.chars().collect();
    let letter = |i: Option<usize>| {
        i.and_then(|i| chars.get(i))
            .is_some_and(|c| c.is_alphabetic())
    };
    (0..chars.len())
        .map(|i| {
            if chars[i] == '-' && (letter(i.checked_sub(1)) || letter(Some(i + 1))) {
                '\u{2010}'
            } else {
                chars[i]
            }
        })
        .collect::<String>()
        .into()
}
