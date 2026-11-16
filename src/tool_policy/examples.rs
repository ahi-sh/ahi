//! Reviewed tool-policy templates, embedded for installed binaries.

/// One shipped policy: its file stem, the text, and what the model gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Example {
    pub name: &'static str,
    pub text: &'static str,
    pub summary: &'static str,
}

/// The default for `ahi tools init`: the policy this repository ships.
pub const DEFAULT_EXAMPLE: &str = "read-all";

/// Shipped policies, in the order `ahi tools examples` prints them. The
/// deliberately invalid `invalid-version.yaml` stays in `examples/tools` for
/// the fail-closed walkthrough and is not offered here.
pub const EXAMPLES: [Example; 9] = [
    Example {
        name: "read-all",
        text: include_str!("../../examples/tools/read-all.yaml"),
        summary: "read, list, find, and search the whole working directory; root .env denied; no prompts",
    },
    Example {
        name: "ask-every-read",
        text: include_str!("../../examples/tools/ask-every-read.yaml"),
        summary: "every read, listing, find, and search in the working directory asks in chat",
    },
    Example {
        name: "sources-only",
        text: include_str!("../../examples/tools/sources-only.yaml"),
        summary: "list the whole working directory; read, find, and search only src and docs",
    },
    Example {
        name: "list-only",
        text: include_str!("../../examples/tools/list-only.yaml"),
        summary: "list directories in the working directory only; reads are denied",
    },
    Example {
        name: "edit-sources",
        text: include_str!("../../examples/tools/edit-sources.yaml"),
        summary: "read the whole working directory; edit under src and tests after a prompt; create under tests",
    },
    Example {
        name: "commands",
        text: include_str!("../../examples/tools/commands.yaml"),
        summary: "read the whole working directory; cargo check and test run; other cargo asks; publish and install denied",
    },
    Example {
        name: "delegate",
        text: include_str!("../../examples/tools/delegate.yaml"),
        summary: "read the whole working directory; hand tasks to the workers labeled rust-ci (after a prompt) and docs; prod denied",
    },
    Example {
        name: "skills",
        text: include_str!("../../examples/tools/skills.yaml"),
        summary: "read the whole working directory; the skills rust-review and release (after a prompt) may be loaded; deploy denied",
    },
    Example {
        name: "deny-all",
        text: include_str!("../../examples/tools/deny-all.yaml"),
        summary: "valid, no approve rule: no tool is advertised",
    },
];

/// The example called `name`, or the names that exist.
pub fn example(name: &str) -> std::result::Result<&'static Example, String> {
    EXAMPLES.iter().find(|e| e.name == name).ok_or_else(|| {
        let names: Vec<&str> = EXAMPLES.iter().map(|e| e.name).collect();
        format!(
            "no example named `{name}`; `ahi tools examples list` prints them: {}",
            names.join(", ")
        )
    })
}
