#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardItem {
    pub id: String,
    pub raw_line: String,
    pub preview: String,
    pub visible_preview: String,
    pub search_text: String,
    pub kind: ClipboardKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardKind {
    Text,
    Image,
    Binary,
}

impl ClipboardItem {
    pub fn parse(raw_line: &str, max_preview_chars: usize) -> Option<Self> {
        let raw_line = raw_line.trim_end_matches(['\r', '\n']).to_string();
        if raw_line.trim().is_empty() {
            return None;
        }

        let (id, preview) = split_id_preview(&raw_line);
        let id = id.to_string();
        let preview = preview.to_string();
        let kind = classify_preview(&preview);
        let visible_preview = match kind {
            ClipboardKind::Text => {
                clamp_preview(&normalize_whitespace(&preview), max_preview_chars)
            }
            ClipboardKind::Image => image_label(&preview),
            ClipboardKind::Binary => binary_label(&preview),
        };

        let search_text = normalize_search(&format!("{} {}", kind.label(), visible_preview));

        Some(Self {
            id,
            raw_line,
            preview,
            visible_preview,
            search_text,
            kind,
        })
    }

    pub fn mime_type(&self) -> Option<&'static str> {
        let lower = self.preview.to_ascii_lowercase();
        if !lower.contains("[[ binary data") {
            return None;
        }

        if lower.contains(" png") || lower.contains("png ") || lower.ends_with("png ]]") {
            Some("image/png")
        } else if lower.contains(" jpeg") || lower.contains(" jpg") || lower.ends_with("jpg ]]") {
            Some("image/jpeg")
        } else if lower.contains(" gif") {
            Some("image/gif")
        } else if lower.contains(" webp") {
            Some("image/webp")
        } else if lower.contains(" bmp") {
            Some("image/bmp")
        } else if lower.contains(" tiff") || lower.contains(" tif") {
            Some("image/tiff")
        } else {
            None
        }
    }
}

impl ClipboardKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Image => "Image",
            Self::Binary => "Binary",
        }
    }
}

pub fn parse_list(output: &str, max_preview_chars: usize) -> Vec<ClipboardItem> {
    output
        .lines()
        .filter_map(|line| ClipboardItem::parse(line, max_preview_chars))
        .collect()
}

pub fn normalize_search(input: &str) -> String {
    normalize_whitespace(input).to_ascii_lowercase()
}

fn split_id_preview(raw_line: &str) -> (&str, &str) {
    if let Some((id, preview)) = raw_line.split_once('\t') {
        return (id.trim(), preview.trim());
    }

    let mut split = raw_line.splitn(2, char::is_whitespace);
    let id = split.next().unwrap_or(raw_line).trim();
    let preview = split.next().unwrap_or("").trim();
    (id, preview)
}

fn classify_preview(preview: &str) -> ClipboardKind {
    let lower = preview.to_ascii_lowercase();
    if lower.contains("[[ binary data") {
        if [
            " png", " jpg", " jpeg", " gif", " webp", " bmp", " tiff", " tif",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
        {
            ClipboardKind::Image
        } else {
            ClipboardKind::Binary
        }
    } else {
        ClipboardKind::Text
    }
}

fn normalize_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len().min(256));
    let mut previous_space = false;

    for ch in input.chars() {
        if ch.is_whitespace() {
            if !previous_space {
                output.push(' ');
                previous_space = true;
            }
        } else {
            output.push(ch);
            previous_space = false;
        }
    }

    output.trim().to_string()
}

fn clamp_preview(preview: &str, max_chars: usize) -> String {
    if preview.chars().count() <= max_chars {
        return preview.to_string();
    }

    let keep = max_chars.saturating_sub(3).max(1);
    let mut output: String = preview.chars().take(keep).collect();
    output.push_str("...");
    output
}

fn image_label(preview: &str) -> String {
    image_dimensions(preview)
        .map(|dimensions| format!("Image {}", dimensions))
        .unwrap_or_else(|| "Image".into())
}

fn binary_label(preview: &str) -> String {
    binary_size(preview)
        .map(|size| format!("Binary {}", size))
        .unwrap_or_else(|| "Binary data".into())
}

fn image_dimensions(preview: &str) -> Option<&str> {
    preview
        .split_whitespace()
        .find(|part| part.contains('x') && part.chars().all(|ch| ch.is_ascii_digit() || ch == 'x'))
}

fn binary_size(preview: &str) -> Option<String> {
    let parts: Vec<_> = preview.split_whitespace().collect();
    for window in parts.windows(2) {
        let unit = window[1];
        if matches!(unit, "B" | "KiB" | "MiB" | "GiB" | "KB" | "MB" | "GB")
            && window[0].chars().all(|ch| ch.is_ascii_digit() || ch == '.')
        {
            return Some(format!("{} {}", window[0], unit));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tab_separated_text() {
        let item = ClipboardItem::parse("8108\tfoo\nbar", 32).unwrap();
        assert_eq!(item.id, "8108");
        assert_eq!(item.visible_preview, "foo bar");
        assert_eq!(item.kind, ClipboardKind::Text);
    }

    #[test]
    fn classifies_image_binary() {
        let item =
            ClipboardItem::parse("8107    [[ binary data 303 KiB png 542x422 ]]", 32).unwrap();
        assert_eq!(item.kind, ClipboardKind::Image);
        assert_eq!(item.visible_preview, "Image 542x422");
        assert_eq!(item.mime_type(), Some("image/png"));
    }

    #[test]
    fn clamps_text_preview() {
        let item = ClipboardItem::parse("1\tabcdef", 4).unwrap();
        assert_eq!(item.visible_preview, "a...");
    }
}
