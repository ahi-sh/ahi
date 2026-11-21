//! The model chooses a task, not a server, credentials, cwd or parent identity.
use super::{
    broker::{Request, Source},
    *,
};
use crate::tool_policy::ExecutionMode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(crate) const SELECTOR: &str = "local:rex-scratch";
pub(crate) fn selector() -> &'static [String] {
    static KEY: std::sync::LazyLock<[String; 1]> =
        std::sync::LazyLock::new(|| [SELECTOR.to_owned()]);
    &*KEY
}
pub(crate) fn granted(mode: &ExecutionMode, policy: &crate::tool_policy::ActionPolicy) -> bool {
    use crate::tool_policy::Verdict;
    mode.allows("delegate")
        && matches!(policy.evaluate("delegate","",selector()),Verdict::Approve{rule_id} if matches!(mode,ExecutionMode::FullAccess|ExecutionMode::ReadWrite) || policy.rule(&rule_id).and_then(|r|r.argv.as_deref())==Some(selector()))
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Task {
    local: Placement,
    prompt: String,
    #[serde(default)]
    pub(crate) follow_ups: Vec<String>,
    timeout: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Placement {
    profile: String,
    mode: Option<ExecutionMode>,
}
impl Task {
    pub(crate) fn parse(v: Value) -> std::result::Result<Self, &'static str> {
        let t: Self =
            serde_json::from_value(v).map_err(|_| "invalid local delegation arguments")?;
        if t.local.profile != "rex-scratch" || t.prompt.trim().is_empty() || t.timeout == Some(0) {
            return Err("expected profile rex-scratch, a prompt and positive timeout");
        }
        Ok(t)
    }
    pub(crate) fn schema() -> Value {
        json!({"type":"object","additionalProperties":false,"required":["local","prompt"],"properties":{"local":{"type":"object","additionalProperties":false,"required":["profile"],"properties":{"profile":{"type":"string","enum":["rex-scratch"]},"mode":{"type":"string","description":"Omit to inherit; request plan for fewer privileges"}}},"prompt":{"type":"string"},"follow_ups":{"type":"array","maxItems":15,"items":{"type":"string"}},"timeout":{"type":"integer","minimum":1}}})
    }
}

#[derive(Clone)]
pub(crate) struct Profile {
    pub dirs: Directories,
    pub socket: PathBuf,
    pub session: String,
    pub run: String,
    pub target: Target,
    pub gateway: String,
    pub model: String,
}
impl Profile {
    pub(crate) fn execute(
        &self,
        task: &Task,
        cancel: &AtomicBool,
        deadline: Option<Instant>,
    ) -> std::result::Result<crate::tools::Executed, crate::tools::ToolFailure> {
        let start = Instant::now();
        let timeout_secs = task
            .timeout
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(MAX_TIMEOUT_SECS)
            .min(deadline.map_or(u64::MAX, |d| d.saturating_duration_since(start).as_secs()));
        if timeout_secs == 0 || cancel.load(Ordering::Relaxed) {
            return Err("parent is stopping".into());
        }
        let id = crate::session::uuid();
        let request = Request {
            id: id.clone(),
            source: Source::Parent {
                session: self.session.clone(),
                run: self.run.clone(),
            },
            mode: task.local.mode.clone(),
            spec: Spec {
                gateway: self.gateway.clone(),
                model: self.model.clone(),
                thinking: "off".into(),
                prompt: task.prompt.clone(),
                timeout_secs,
                target: self.target.clone(),
            },
            follow_ups: task.follow_ups.clone(),
            preview: false,
        };
        let mut c = crate::service::client::Client::listening(&self.dirs, &self.socket)
            .map_err(|_| "no listening local service")?;
        let launched = c.call("child.admit", json!(request));
        let (status, text) = match launched {
            Err(e) if matches!(e.kind, "policy" | "usage" | "config") => return Err(
                "local child refused before launch: check the parent mode, target and task limits"
                    .into(),
            ),
            Err(_) => (
                "unknown".into(),
                "launch not established; reconcile this assignment before retrying".into(),
            ),
            Ok(_) => loop {
                let status =
                    super::status(&self.dirs, &id).map_err(|_| "assignment status unknown")?;
                if let Some(s) = status["result"]["status"].as_str() {
                    break (
                        s.to_owned(),
                        status["result"]["text"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    );
                }
                if cancel.load(Ordering::Relaxed)
                    || start.elapsed() >= Duration::from_secs(timeout_secs)
                {
                    let mut receipt =
                        super::load(&self.dirs, &id).map_err(|_| "lost child receipt")?;
                    let _ = super::cancel(&self.dirs, &mut receipt);
                    break (
                        "unknown".into(),
                        "stop requested; completion is not established".into(),
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            },
        };
        let effects = "unknown"; // No parent-checkout effects; scratch may contain writes.
        let content =
            json!({"assignment":id,"status":status,"effects":effects,"text":text}).to_string();
        Ok(crate::tools::Executed {
            content,
            delegation: Some(crate::tools::delegate::DelegationReport {
                via: vec!["ahi-service".into()],
                worker: "local:rex-scratch".into(),
                session: id.clone(),
                run_id: id,
                status,
                effects: effects.into(),
                duration_ms: start.elapsed().as_millis(),
            }),
            ..Default::default()
        })
    }
}
