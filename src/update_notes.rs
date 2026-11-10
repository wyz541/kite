#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateNoteSection {
    pub title: String,
    pub items: Vec<String>,
    internal: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateNotesContent {
    pub summary: Option<String>,
    pub sections: Vec<UpdateNoteSection>,
}

pub fn parse_release_notes(notes: &str) -> UpdateNotesContent {
    let mut summary = Vec::new();
    let mut sections: Vec<UpdateNoteSection> = Vec::new();
    let mut in_code_fence = false;
    let mut in_html_comment = false;

    for raw_line in notes.lines() {
        let line = raw_line.trim();
        if line.starts_with("```") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence {
            continue;
        }
        if line.contains("<!--") {
            in_html_comment = true;
        }
        if in_html_comment {
            if line.contains("-->") {
                in_html_comment = false;
            }
            continue;
        }
        if line.is_empty() || line == "---" || line.starts_with("<!--") {
            continue;
        }

        if let Some((level, title)) = markdown_heading(line) {
            let title = clean_release_note_text(title);
            if title.is_empty() || level == 1 {
                continue;
            }
            if level > 2 {
                if let Some(section) = sections.last_mut() {
                    section.items.push(title);
                }
                continue;
            }
            sections.push(UpdateNoteSection {
                internal: is_internal_release_section(&title),
                title,
                items: Vec::new(),
            });
            continue;
        }

        let text = markdown_list_item(line)
            .map(clean_release_note_text)
            .unwrap_or_else(|| clean_release_note_text(line));
        if text.is_empty() {
            continue;
        }

        if let Some(section) = sections.last_mut() {
            section.items.push(text);
        } else {
            summary.push(text);
        }
    }

    let has_external_section = sections.iter().any(|section| !section.internal);
    if has_external_section {
        sections.retain(|section| !section.internal);
    } else {
        for section in &mut sections {
            section.internal = false;
        }
    }
    sections.retain(|section| !section.title.is_empty() || !section.items.is_empty());

    UpdateNotesContent {
        summary: (!summary.is_empty()).then(|| summary.join(" ")),
        sections,
    }
}

pub fn published_date_label(published_at: Option<&str>) -> Option<String> {
    let value = published_at?.trim();
    let date = value.get(..10)?;
    let valid = date.len() == 10
        && date.chars().enumerate().all(|(index, character)| {
            if matches!(index, 4 | 7) {
                character == '-'
            } else {
                character.is_ascii_digit()
            }
        });
    valid.then(|| date.to_string())
}

fn markdown_heading(line: &str) -> Option<(usize, &str)> {
    let level = line
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = line.get(level..)?;
    if !rest.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }
    let title = rest.trim();
    (!title.is_empty()).then_some((level, title))
}

fn markdown_list_item(line: &str) -> Option<&str> {
    if let Some(item) = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
    {
        return Some(item);
    }
    let (number, item) = line.split_once(". ")?;
    (!number.is_empty() && number.chars().all(|character| character.is_ascii_digit()))
        .then_some(item)
}

fn clean_release_note_text(text: &str) -> String {
    let mut text = text.trim();
    for prefix in ["[x] ", "[X] ", "[ ] ", "> "] {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest;
            break;
        }
    }

    let characters: Vec<char> = text.chars().collect();
    let mut cleaned = String::with_capacity(text.len());
    let mut index = 0;
    while index < characters.len() {
        if characters[index] == '`' {
            index += 1;
            continue;
        }
        if matches!(characters[index], '*' | '_')
            && characters.get(index + 1) == Some(&characters[index])
        {
            index += 2;
            continue;
        }
        if characters[index] == '['
            && let Some(label_end) = characters[index + 1..]
                .iter()
                .position(|character| *character == ']')
                .map(|offset| index + 1 + offset)
            && characters.get(label_end + 1) == Some(&'(')
            && let Some(url_end) = characters[label_end + 2..]
                .iter()
                .position(|character| *character == ')')
                .map(|offset| label_end + 2 + offset)
        {
            cleaned.extend(characters[index + 1..label_end].iter());
            index = url_end + 1;
            continue;
        }
        cleaned.push(characters[index]);
        index += 1;
    }
    cleaned.trim().to_string()
}

fn is_internal_release_section(title: &str) -> bool {
    let title = title.trim().to_ascii_lowercase();
    [
        "verification",
        "validation",
        "testing",
        "tests",
        "test plan",
        "release checklist",
        "release verification",
        "build",
        "install",
        "installation",
        "platform note",
        "验证",
        "测试",
        "构建",
        "安装",
        "平台说明",
    ]
    .iter()
    .any(|marker| title.starts_with(marker))
}

#[cfg(test)]
mod tests {
    use super::{parse_release_notes, published_date_label};

    #[test]
    fn empty_notes_have_no_invented_content() {
        let notes = parse_release_notes(" \n ");

        assert!(notes.summary.is_none());
        assert!(notes.sections.is_empty());
    }

    #[test]
    fn product_sections_remain_and_internal_checks_are_hidden() {
        let notes = parse_release_notes(
            "# Kite v0.6.71\n\
             Kite improves the settings experience.\n\
             ## Windows settings\n\
             - Add a `dedicated` settings window.\n\
             - Persist **layout** preferences.\n\
             ## Verification\n\
             - Windows Clippy check.\n",
        );

        assert_eq!(
            notes.summary.as_deref(),
            Some("Kite improves the settings experience.")
        );
        assert_eq!(notes.sections.len(), 1);
        assert_eq!(notes.sections[0].title, "Windows settings");
        assert_eq!(notes.sections[0].items.len(), 2);
        assert_eq!(
            notes.sections[0].items[0],
            "Add a dedicated settings window."
        );
        assert_eq!(notes.sections[0].items[1], "Persist layout preferences.");
    }

    #[test]
    fn internal_only_notes_remain_visible() {
        let notes = parse_release_notes("## Verification\n- Windows release build.\n");

        assert_eq!(notes.sections.len(), 1);
        assert_eq!(notes.sections[0].title, "Verification");
        assert_eq!(notes.sections[0].items[0], "Windows release build.");
    }

    #[test]
    fn shipped_release_notes_render_as_product_content() {
        let notes = parse_release_notes(include_str!("../docs/RELEASE_NOTES_v0.6.73.md"));
        let titles = notes
            .sections
            .iter()
            .map(|section| section.title.as_str())
            .collect::<Vec<_>>();

        assert!(titles.contains(&"Update window"));
        assert!(titles.contains(&"Release notes parser"));
        assert!(!titles.contains(&"Verification"));
        assert!(!titles.contains(&"Install"));
        assert!(!titles.contains(&"Platform note"));
        assert!(
            notes
                .sections
                .iter()
                .flat_map(|section| &section.items)
                .all(|item| !item.starts_with("##")
                    && !item.starts_with("- ")
                    && !item.contains('`'))
        );
    }

    #[test]
    fn markdown_links_keep_only_visible_text() {
        let notes = parse_release_notes("## Details\n- Read [the guide](https://example.com).\n");

        assert_eq!(notes.sections[0].items[0], "Read the guide.");
    }

    #[test]
    fn release_date_accepts_iso_timestamps_only() {
        assert_eq!(
            published_date_label(Some("2026-11-10T12:34:56Z")).as_deref(),
            Some("2026-11-10")
        );
        assert!(published_date_label(Some("10 November 2026")).is_none());
        assert!(published_date_label(None).is_none());
    }
}
