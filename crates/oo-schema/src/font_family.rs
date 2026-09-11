/// Resolve the first family in a CSS font stack for native Office font fields.
/// These fields accept one family name, without CSS quotes or fallback entries.
pub fn primary_font_family(stack: &str) -> String {
    let mut result = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in stack.trim().chars() {
        if escaped {
            result.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            } else {
                result.push(character);
            }
        } else if character == '\'' || character == '"' {
            quote = Some(character);
        } else if character == ',' {
            break;
        } else {
            result.push(character);
        }
    }
    result.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::primary_font_family;

    #[test]
    fn office_family_is_not_a_css_fallback_stack() {
        assert_eq!(
            primary_font_family("\"Noto Sans SC\", sans-serif"),
            "Noto Sans SC"
        );
        assert_eq!(primary_font_family("Lora, serif"), "Lora");
        assert_eq!(
            primary_font_family("'Font, Display', serif"),
            "Font, Display"
        );
        assert_eq!(primary_font_family(" 思源宋体 "), "思源宋体");
    }
}
