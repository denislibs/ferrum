//! Имена начертаний. В документе шрифт — семейство и имя начертания, как
//! `fontName` в Figma («Semi Bold Italic»); у файлов те же начертания пишутся
//! по-разному («SemiBold Italic», «Semibold Italic»). Сравнение — по ключу без
//! пробелов, дефисов и регистра.

/// Ключ имени начертания: строчные буквы и цифры без разделителей.
pub fn style_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Вес и курсив по имени начертания, если о шрифте больше ничего не известно
/// (запасной Inter для отсутствующего шрифта): «Bold Italic» → 700 и курсив.
pub fn guess_attrs(name: &str) -> (f32, bool) {
    let key = style_key(name);
    let italic = key.contains("italic") || key.contains("oblique");
    // Сначала длинные: «extrabold» раньше «bold», «semibold» раньше «bold».
    const WEIGHTS: [(&str, f32); 14] = [
        ("hairline", 100.0),
        ("thin", 100.0),
        ("extralight", 200.0),
        ("ultralight", 200.0),
        ("semilight", 350.0),
        ("light", 300.0),
        ("medium", 500.0),
        ("semibold", 600.0),
        ("demibold", 600.0),
        ("extrabold", 800.0),
        ("ultrabold", 800.0),
        ("bold", 700.0),
        ("black", 900.0),
        ("heavy", 900.0),
    ];
    let weight = WEIGHTS
        .iter()
        .find(|(w, _)| key.contains(w))
        .map_or(400.0, |(_, v)| *v);
    (weight, italic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ignore_spaces_and_case() {
        assert_eq!(style_key("Semi Bold Italic"), style_key("SemiBold Italic"));
        assert_eq!(style_key("Extra-Light"), "extralight");
    }

    #[test]
    fn guesses_figma_style_names() {
        assert_eq!(guess_attrs("Regular"), (400.0, false));
        assert_eq!(guess_attrs("Italic"), (400.0, true));
        assert_eq!(guess_attrs("Semi Bold Italic"), (600.0, true));
        assert_eq!(guess_attrs("Extra Bold"), (800.0, false));
        assert_eq!(guess_attrs("Bold"), (700.0, false));
        assert_eq!(guess_attrs("Extra Light"), (200.0, false));
        assert_eq!(guess_attrs("Black Italic"), (900.0, true));
    }
}
