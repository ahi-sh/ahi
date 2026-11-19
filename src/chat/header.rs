//! The startup header: a text wordmark, the version, the model, and the
//! first keys to know. It is a notice, so it never reaches a model and
//! `/clear` removes it like any other row.

/// Two rows of block glyphs spelling "ahi". The formatter recognises a
/// notice that begins with the first row and paints both rows in the accent.
pub(crate) const WORDMARK: [&str; 2] = ["▄▀█ █░█ █", "█▀█ █▀█ █"];

/// Setup hints are centered by the transcript formatter at its actual width.
pub(crate) const SETUP_LINES: [&str; 7] = [
    "Run in your workspace terminal, then restart ahi tui:",
    "Wizard: ahi tools init --interactive --write",
    "Read-only: ahi tools init --example read-all --write",
    "Permissive: ahi tools init --example permissive --write",
    "Permissive allows host commands without prompts; not sandboxed.",
    "More policies: ahi tools examples list",
    "Preview any policy by omitting --write.",
];

/// Build the header text for the given facts. Below 60 columns the wordmark
/// is dropped so the first message stays on a small screen.
pub(crate) fn text(
    width: usize,
    model: &str,
    gateway: &str,
    thinking: &str,
    directory: &str,
    tools_configured: bool,
) -> String {
    let version = crate::VERSION;
    let model_line = format!("{model} via {gateway} · thinking {thinking}");
    let place_line = format!(
        "{directory} · {}",
        if tools_configured {
            "tools enabled · /mode controls access"
        } else {
            "no ahi.tools.yaml: tools off"
        }
    );
    let keys_line = "/ for commands · Ctrl+S selects · /mouse toggles native selection";
    let setup = if tools_configured {
        String::new()
    } else {
        format!("\n\n{}", SETUP_LINES.join("\n"))
    };
    if width < 60 {
        return format!("ahi {version} · {model_line}\n{place_line}\n{keys_line}{setup}");
    }
    let indent = " ".repeat(WORDMARK[0].chars().count() + 3);
    format!(
        "{}   ahi {version}\n{}   {model_line}\n{indent}{place_line}\n{indent}{keys_line}{setup}",
        WORDMARK[0], WORDMARK[1]
    )
}

pub(crate) fn skills_text(
    catalog: &crate::skills::Catalog,
    policy: Option<&crate::tool_policy::ActionPolicy>,
) -> String {
    if catalog.skills.is_empty() {
        return "Skills: none discovered".into();
    }
    let mut lines = vec!["Skills (loaded on demand):".to_string()];
    for skill in &catalog.skills {
        let status = if policy.is_some_and(|policy| {
            !matches!(
                policy.evaluate_skill(&skill.id()),
                crate::tool_policy::Verdict::Deny { .. }
            )
        }) {
            if skill.disable_model_invocation {
                "manual only"
            } else {
                "available"
            }
        } else {
            "not approved"
        };
        lines.push(format!("Skill: {} ({status})", skill.id()));
    }
    lines.join("\n")
}

pub(crate) fn with_skills(header: String, skills: &str) -> String {
    match header.split_once("\n\n") {
        Some((intro, setup)) => format!("{intro}\n\n{skills}\n\n{setup}"),
        None => format!("{header}\n\n{skills}"),
    }
}

pub(crate) fn is_header(text: &str) -> bool {
    text.starts_with(WORDMARK[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_precede_setup_and_remain_when_tools_are_configured() {
        let skills = skills_text(&crate::skills::Catalog::default(), None);
        assert_eq!(skills, "Skills: none discovered");
        for configured in [false, true] {
            let header = with_skills(text(100, "m", "g", "off", "repo", configured), &skills);
            assert_eq!(header.lines().nth(5), Some("Skills: none discovered"));
            if configured {
                assert!(!header.contains("Wizard:"));
            } else {
                assert_eq!(
                    header.lines().nth(8),
                    Some("Wizard: ahi tools init --interactive --write")
                );
            }
        }
    }

    #[test]
    fn header_fits_wide_and_narrow_screens() {
        let wide = text(100, "gpt-5.1", "openai", "medium", "~/w/ahi on main", true);
        assert!(is_header(&wide));
        assert_eq!(wide.lines().count(), 4);
        assert!(wide.contains(&format!("ahi {}", crate::VERSION)));
        assert!(wide.contains("gpt-5.1 via openai · thinking medium"));
        let narrow = text(50, "gpt-5.1", "openai", "medium", "~/w/ahi", false);
        assert!(!is_header(&narrow));
        assert_eq!(narrow.lines().count(), 11);
        assert!(narrow.contains("Wizard: ahi tools init --interactive --write"));
        assert!(narrow.contains("Read-only: ahi tools init --example read-all --write"));
        assert!(narrow.contains("Permissive: ahi tools init --example permissive --write"));
        assert!(!wide.contains("ahi tools init"));
        assert!(narrow.contains("tools off"));
    }
}
