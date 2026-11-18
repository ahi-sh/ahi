//! Qualified identities at the CLI, wire, tool and policy boundaries.

pub(crate) fn valid_id(id: &str) -> bool {
    id.split_once('/').is_some_and(|(scope, name)| {
        matches!(scope, "repo" | "operator") && crate::precommit::policy::valid_rule_id(name)
    })
}

pub(crate) fn parse_id(id: &str) -> Result<String, String> {
    valid_id(id)
        .then(|| id.to_owned())
        .ok_or_else(|| "expected repo/<skill-name> or operator/<skill-name>".into())
}

pub(crate) fn query_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/')
}
