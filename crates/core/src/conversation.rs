//! A session's events as a conversation a person reads: what they asked, what Claude said,
//! thought and did, and what the daemon knows. The transcript carries the conversation;
//! hooks mostly repeat it and are shown only where they add something (EXP-0002, EXP-0010).
//! Every shape Claude Code writes into a user line (slash commands and their output, `!`
//! shell commands, background-task notices, system reminders, peer messages) is turned into
//! something readable or dropped, so the client never shows raw markup.

use std::collections::BTreeMap;

use saarthi_proto::event::v1::{Event, EventSource};
use serde_json::Value;

/// How much of a tool result, a thinking block or a tool's detail is kept: the client folds
/// them until tapped, so a screen or two is enough.
const RESULT_CHARS: usize = 4000;
const THINKING_CHARS: usize = 2000;
const DETAIL_CHARS: usize = 4000;
/// An edit's text, a multi-line shell command (the edit itself when files are written
/// through heredocs) and the diff of the files a call changed are kept nearly whole: the
/// app shows their first lines and the rest full screen.
const EDIT_CHARS: usize = 200_000;
/// The one-line input of a tool call.
const HEADLINE_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum Role {
    User,
    Assistant,
    /// Claude's thinking, folded by default.
    Thinking,
    /// Claude calling a tool: `tool` names it, `text` is the one input a person cares about,
    /// `detail` the rest worth a look (an edit's diff, a file's content, an agent's prompt).
    Tool,
    /// What a tool returned, clipped; `error` when the tool failed.
    ToolResult,
    /// Claude Code's own notices: slash commands, compaction, held messages, notifications.
    System,
    /// Facts only the daemon knows, such as an interrupted turn.
    Daemon,
    /// A turn's time and tokens, after Claude's reply: "14 s · 3.2k in · 410 out · $0.01".
    Turn,
}

/// Token totals for one model over a session, as the daemon counts them (once per message
/// id, subagents included), with a cost estimate from [`price`].
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
pub struct ModelTotals {
    pub model: String,
    pub messages: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// An estimate from a price table by model family; 0 for an unknown model.
    pub cost_usd: f64,
}

/// Dollars per million tokens: input, output, cache read, cache write. By family, since
/// model ids carry dates; kept here until the hub carries a table of its own (ADR-0030).
fn price(model: &str) -> Option<[f64; 4]> {
    let m = model.to_ascii_lowercase();
    if m.contains("haiku") {
        Some([1.0, 5.0, 0.1, 1.25])
    } else if m.contains("sonnet") {
        Some([3.0, 15.0, 0.3, 3.75])
    } else if m.contains("opus") {
        Some([15.0, 75.0, 1.5, 18.75])
    } else if m.contains("fable") || m.contains("mythos") {
        Some([15.0, 75.0, 1.5, 18.75])
    } else {
        None
    }
}

impl ModelTotals {
    fn from_daemon(model: &str, v: &Value) -> Self {
        let n = |k: &str| v[k].as_u64().unwrap_or(0);
        let mut t = ModelTotals {
            model: model.to_string(),
            messages: n("messages"),
            input_tokens: n("input_tokens"),
            output_tokens: n("output_tokens"),
            cache_read_tokens: n("cache_read_input_tokens"),
            cache_write_tokens: n("cache_creation_input_tokens"),
            cost_usd: 0.0,
        };
        t.cost_usd = t.cost();
        t
    }

    fn cost(&self) -> f64 {
        match price(&self.model) {
            Some([i, o, r, w]) => {
                (self.input_tokens as f64 * i
                    + self.output_tokens as f64 * o
                    + self.cache_read_tokens as f64 * r
                    + self.cache_write_tokens as f64 * w)
                    / 1e6
            }
            None => 0.0,
        }
    }

    fn minus(&self, before: &ModelTotals) -> ModelTotals {
        let mut t = ModelTotals {
            model: self.model.clone(),
            messages: self.messages.saturating_sub(before.messages),
            input_tokens: self.input_tokens.saturating_sub(before.input_tokens),
            output_tokens: self.output_tokens.saturating_sub(before.output_tokens),
            cache_read_tokens: self.cache_read_tokens.saturating_sub(before.cache_read_tokens),
            cache_write_tokens: self.cache_write_tokens.saturating_sub(before.cache_write_tokens),
            cost_usd: 0.0,
        };
        t.cost_usd = t.cost();
        t
    }
}

/// The daemon's usage totals by model, as a `usage` event's payload carries them.
pub fn totals(usage_json: &str) -> Vec<ModelTotals> {
    let v: Value = serde_json::from_str(usage_json).unwrap_or_default();
    v.as_object()
        .into_iter()
        .flatten()
        .map(|(m, t)| ModelTotals::from_daemon(m, t))
        .collect()
}

/// The running state a session's stream needs beyond one event: the daemon's usage
/// totals, and where the current turn started, so each `turn_duration` line becomes a
/// `Turn` item with that turn's time and tokens (EXP-20261025-25f8mm).
#[derive(Debug, Default)]
pub struct Conversation {
    totals: BTreeMap<String, ModelTotals>,
    turn_start: BTreeMap<String, ModelTotals>,
    /// Whether `totals` changed since the last time they were taken.
    changed: bool,
    /// How full the context was when the node last asked, not yet taken (ADR-0061).
    context: Option<ContextUsage>,
    /// The session's queued prompts when the node last reported them, not yet taken.
    queue: Option<Vec<QueuedPrompt>>,
}

/// A prompt the node holds until its session is next idle (phase PHS-20261117-f22kxe item 7).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct QueuedPrompt {
    pub id: String,
    pub text: String,
    pub time_ms: i64,
}

/// How full a spawned session's context is, as Claude Code reports it after a turn: the
/// tokens in use and the window before it compacts.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ContextUsage {
    pub used_tokens: u64,
    pub max_tokens: u64,
}

impl Conversation {
    /// A conversation resumed after a cached part: the totals so far are the start of the
    /// next turn.
    pub fn seeded(totals: Vec<ModelTotals>) -> Self {
        let map: BTreeMap<String, ModelTotals> = totals.into_iter().map(|t| (t.model.clone(), t)).collect();
        Self {
            turn_start: map.clone(),
            totals: map,
            changed: false,
            context: None,
            queue: None,
        }
    }

    /// The items one event shows as, with the turn and usage state applied.
    pub fn push(&mut self, e: &Event) -> Vec<Item> {
        if e.source() == EventSource::Daemon && e.kind == "context" {
            let v: Value = serde_json::from_str(&e.payload_json).unwrap_or_default();
            if let (Some(used), Some(max)) = (v["total_tokens"].as_u64(), v["max_tokens"].as_u64()) {
                self.context = Some(ContextUsage { used_tokens: used, max_tokens: max });
            }
            return Vec::new();
        }
        if e.source() == EventSource::Daemon && e.kind == "queue" {
            let v: Value = serde_json::from_str(&e.payload_json).unwrap_or_default();
            let items = v["items"].as_array().into_iter().flatten();
            self.queue = Some(
                items
                    .map(|q| QueuedPrompt {
                        id: q["id"].as_str().unwrap_or_default().to_string(),
                        text: q["text"].as_str().unwrap_or_default().to_string(),
                        time_ms: q["time_ms"].as_i64().unwrap_or_default(),
                    })
                    .collect(),
            );
            return Vec::new();
        }
        if e.source() == EventSource::Daemon && e.kind == "usage" {
            let v: Value = serde_json::from_str(&e.payload_json).unwrap_or_default();
            if let Some(map) = v.as_object() {
                self.totals = map
                    .iter()
                    .map(|(m, t)| (m.clone(), ModelTotals::from_daemon(m, t)))
                    .collect();
                self.changed = true;
            }
            return Vec::new();
        }
        if e.source() == EventSource::Transcript && e.kind == "system" {
            let v: Value = serde_json::from_str(&e.payload_json).unwrap_or_default();
            if v["subtype"] == "turn_duration" {
                let ms = v["durationMs"].as_u64().unwrap_or(0);
                let spent: Vec<ModelTotals> = self
                    .totals
                    .values()
                    .map(|t| t.minus(self.turn_start.get(&t.model).unwrap_or(&ModelTotals::default())))
                    .filter(|t| t.messages > 0)
                    .collect();
                self.turn_start = self.totals.clone();
                return vec![Item {
                    seq: e.seq,
                    time_ms: written(&v).unwrap_or(e.time_ms),
                    role: Role::Turn,
                    tool: String::new(),
                    text: turn_line(ms, &spent),
                    detail: None,
                    diff: None,
                    error: false,
                }];
            }
        }
        items(e)
    }

    /// How full the context is, when the node reported it since the last call.
    pub fn take_context(&mut self) -> Option<ContextUsage> {
        self.context.take()
    }

    /// The session's queued prompts, when the node reported them since the last call.
    pub fn take_queue(&mut self) -> Option<Vec<QueuedPrompt>> {
        self.queue.take()
    }

    /// The session's totals by model when they changed since the last call.
    pub fn take_totals(&mut self) -> Option<Vec<ModelTotals>> {
        if !self.changed {
            return None;
        }
        self.changed = false;
        Some(self.totals.values().cloned().collect())
    }
}

/// "14 s · 22 in · 410 out · 48k cached · $0.01", with the words `/cost` uses: "in" is
/// uncached input only; the cached context is its own figure.
fn turn_line(ms: u64, spent: &[ModelTotals]) -> String {
    let mut parts = vec![duration(ms)];
    let input: u64 = spent.iter().map(|t| t.input_tokens).sum();
    let output: u64 = spent.iter().map(|t| t.output_tokens).sum();
    let cached: u64 = spent.iter().map(|t| t.cache_read_tokens + t.cache_write_tokens).sum();
    let cost: f64 = spent.iter().map(|t| t.cost_usd).sum();
    if input + output + cached > 0 {
        parts.push(format!("{} in", thousands(input)));
        parts.push(format!("{} out", thousands(output)));
    }
    if cached > 0 {
        parts.push(format!("{} cached", thousands(cached)));
    }
    if cost > 0.0 {
        parts.push(dollars(cost));
    }
    parts.join(" · ")
}

pub fn duration(ms: u64) -> String {
    match ms {
        0..=999 => format!("{ms} ms"),
        1000..=59_999 => format!("{:.1} s", ms as f64 / 1000.0),
        _ => format!("{} min {} s", ms / 60_000, (ms % 60_000) / 1000),
    }
}

pub fn thousands(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1000..=999_999 => format!("{:.1}k", n as f64 / 1000.0),
        _ => format!("{:.2}M", n as f64 / 1e6),
    }
}

pub fn dollars(usd: f64) -> String {
    if usd < 0.01 {
        format!("${usd:.3}")
    } else {
        format!("${usd:.2}")
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Item {
    pub seq: u64,
    pub time_ms: i64,
    pub role: Role,
    /// The tool's short name for a `Tool` item; empty otherwise.
    pub tool: String,
    pub text: String,
    /// What a tap reveals, when there is more than the headline.
    pub detail: Option<String>,
    /// For a tool result: the unified diff of the files the call changed, as Claude Code
    /// computes it (`toolUseResult.bashEditDiff` for Bash, `structuredPatch` for an edit),
    /// one `=== path` header per file, hunks as `@@` lines and ` `, `-`, `+` lines.
    pub diff: Option<String>,
    pub error: bool,
}

/// When Claude Code wrote a transcript line, from its own `timestamp`; the event's time is
/// when the node read it, which after a restart is when it read the transcript again.
fn written(v: &Value) -> Option<i64> {
    let t: jiff::Timestamp = v["timestamp"].as_str()?.parse().ok()?;
    Some(t.as_millisecond())
}

/// The items one event shows as; most events show as none or one.
pub fn items(e: &Event) -> Vec<Item> {
    let v: Value = serde_json::from_str(&e.payload_json).unwrap_or_default();
    let time_ms = written(&v).unwrap_or(e.time_ms);
    let item = |role, text: String| Item {
        seq: e.seq,
        time_ms,
        role,
        tool: String::new(),
        text,
        detail: None,
        diff: None,
        error: false,
    };
    let mut out = Vec::new();
    match e.source() {
        EventSource::Transcript => match e.kind.as_str() {
            "user" => {
                let meta = v["isMeta"].as_bool().unwrap_or(false);
                match &v["message"]["content"] {
                    Value::String(s) => out.extend(user_text(s, meta).map(|(r, t)| item(r, t))),
                    Value::Array(blocks) => {
                        for b in blocks {
                            match b["type"].as_str() {
                                Some("text") => {
                                    out.extend(user_text(&text(&b["text"]), meta).map(|(r, t)| item(r, t)))
                                }
                                Some("tool_result") => {
                                    let mut i = item(Role::ToolResult, clip(&result_text(&b["content"]), RESULT_CHARS));
                                    i.error = b["is_error"].as_bool().unwrap_or(false);
                                    i.diff = changed_files(&v["toolUseResult"]);
                                    out.push(i);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            "assistant" => {
                for b in v["message"]["content"].as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("text") if !text(&b["text"]).trim().is_empty() => {
                            out.push(item(Role::Assistant, text(&b["text"])))
                        }
                        Some("thinking") if !text(&b["thinking"]).trim().is_empty() => {
                            out.push(item(Role::Thinking, clip(text(&b["thinking"]).trim(), THINKING_CHARS)))
                        }
                        Some("tool_use") => {
                            let (name, headline, detail) = tool_use(b);
                            let mut i = item(Role::Tool, headline);
                            i.tool = name;
                            i.detail = detail;
                            out.push(i);
                        }
                        _ => {}
                    }
                }
            }
            "system" => {
                let content = v["content"].as_str().unwrap_or_default();
                match v["subtype"].as_str().unwrap_or_default() {
                    "local_command" => out.extend(slash_command(content).map(|t| item(Role::System, t))),
                    "compact_boundary" => out.push(item(Role::System, "Conversation compacted".into())),
                    // Summaries written for Claude's own use after an absence.
                    "away_summary" => {}
                    _ if !content.is_empty() => out.push(item(Role::System, content.to_string())),
                    _ => {}
                }
            }
            _ => {}
        },
        EventSource::Hook if e.kind == "Notification" => {
            // A permission prompt shows as the pending card and an idle prompt as the
            // session's state; the other notifications (auth, elicitation) are news.
            let routine = matches!(v["notification_type"].as_str(), Some("permission_prompt" | "idle_prompt"));
            if let Some(m) = v["message"].as_str().filter(|_| !routine) {
                out.push(item(Role::System, m.to_string()));
            }
        }
        EventSource::Daemon => match e.kind.as_str() {
            "interrupted" => out.push(item(Role::Daemon, "Interrupted".into())),
            "killed" => out.push(item(Role::Daemon, "Killed".into())),
            _ => {}
        },
        _ => {}
    }
    out
}

/// A user line's text as the person should see it, or nothing. Claude Code writes several
/// things into user lines besides what was typed.
fn user_text(s: &str, meta: bool) -> Option<(Role, String)> {
    const PEER: &str = "Another Claude session sent a message:\n";
    if let Some(rest) = s.strip_prefix(PEER) {
        // A prompt sent over the messaging socket (ADR-0014), followed by a paragraph of
        // advice to the model; the person reads only what they sent.
        let sent = rest
            .split("\n\nThis came from another Claude session")
            .next()
            .unwrap_or(rest)
            .trim_end();
        return Some((Role::User, sent.to_string()));
    }
    if meta {
        // Skill bodies, hook output and other text injected for the model.
        return None;
    }
    let t = s.trim_start();
    if let Some(inner) = tagged(t, "task-notification") {
        let summary = tagged(inner, "summary").map(str::trim).filter(|x| !x.is_empty());
        return Some((
            Role::System,
            match summary {
                Some(x) => format!("Background task: {x}"),
                None => "A background task finished".into(),
            },
        ));
    }
    if t.starts_with("<command-name>") {
        return slash_command(t).map(|c| (Role::System, c));
    }
    // A command that prompts the model (a plugin's or the person's own, such as
    // `/yajna:project <brief>`) is written message first: it is what the person typed.
    if t.starts_with("<command-message>") {
        return slash_command(t).map(|c| (Role::User, c));
    }
    if let Some(inner) = tagged(t, "local-command-stdout") {
        let shown = strip_ansi(inner.trim());
        return (!shown.is_empty() && shown != "(no content)")
            .then(|| (Role::ToolResult, clip(&shown, RESULT_CHARS)));
    }
    if t.starts_with("<local-command-caveat>") || t.starts_with("<system-reminder>") {
        return None;
    }
    if let Some(cmd) = tagged(t, "bash-input") {
        return Some((Role::User, format!("! {}", cmd.trim())));
    }
    if t.starts_with("<bash-stdout>") {
        let out = tagged(t, "bash-stdout").unwrap_or_default().trim();
        let err = tagged(t, "bash-stderr").unwrap_or_default().trim();
        let shown = [out, err].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join("\n");
        return (!shown.is_empty()).then(|| (Role::ToolResult, clip(&shown, RESULT_CHARS)));
    }
    if t.starts_with("[Request interrupted by user") {
        return Some((Role::System, "Interrupted".into()));
    }
    // Reminders can be appended to what was typed; the person's words stay.
    let own = without_tagged(s, "system-reminder");
    let own = own.trim();
    (!own.is_empty()).then(|| (Role::User, own.to_string()))
}

/// "/compact", "/model haiku": a slash command typed at the terminal.
fn slash_command(s: &str) -> Option<String> {
    let name = tagged(s, "command-name")?.trim();
    let args = tagged(s, "command-args").map(str::trim).unwrap_or_default();
    Some(if args.is_empty() { name.to_string() } else { format!("{name} {args}") })
}

/// The text inside `<tag>…</tag>` when `s` starts with the opening tag.
fn tagged<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = s.find(&open)? + open.len();
    let end = s[start..].find(&close).map(|i| start + i).unwrap_or(s.len());
    Some(&s[start..end])
}

/// `s` with every `<tag>…</tag>` block removed.
fn without_tagged(s: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(&open) {
        out.push_str(&rest[..i]);
        rest = match rest[i..].find(&close) {
            Some(j) => &rest[i + j + close.len()..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// A tool call as (short name, the one input a person cares about, the rest worth a look).
pub fn tool_use(block: &Value) -> (String, String, Option<String>) {
    let full = block["name"].as_str().unwrap_or("tool");
    // MCP tools are `mcp__<server>__<tool>`; the tool's own name is enough.
    let name = full.rsplit("__").next().unwrap_or(full).to_string();
    let input = &block["input"];
    let s = |k: &str| input[k].as_str().map(str::to_string);
    let (headline, detail) = match full {
        // A command of several lines (a heredoc, a script) is shown whole: it is often the
        // file being written, which the terminal shows and the first line hides.
        "Bash" => {
            let command = s("command").unwrap_or_default();
            let detail = if command.trim().contains('\n') {
                Some(command.trim_end().to_string())
            } else {
                s("description")
            };
            (Some(command), detail)
        }
        "Edit" => (
            s("file_path"),
            Some(diff(&s("old_string").unwrap_or_default(), &s("new_string").unwrap_or_default())),
        ),
        "Write" | "NotebookEdit" => (
            s("file_path"),
            s("content").or_else(|| s("new_source")).map(|c| diff("", &c)),
        ),
        "Read" => (s("file_path"), None),
        "Glob" | "Grep" => (
            s("pattern").map(|p| match s("path") {
                Some(path) => format!("{p} in {path}"),
                None => p,
            }),
            None,
        ),
        "WebFetch" => (s("url"), s("prompt")),
        "WebSearch" => (s("query"), None),
        "Agent" => (s("description"), s("prompt")),
        "AskUserQuestion" => (input["questions"][0]["question"].as_str().map(str::to_string), None),
        "Skill" => (s("skill"), s("args")),
        "SendMessage" => (s("summary").or_else(|| s("message")), s("message")),
        "TodoWrite" => {
            let todos = input["todos"].as_array().map(|t| t.len()).unwrap_or(0);
            let list = input["todos"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| {
                    let mark = match t["status"].as_str() {
                        Some("completed") => "[x]",
                        Some("in_progress") => "[~]",
                        _ => "[ ]",
                    };
                    t["content"].as_str().map(|c| format!("{mark} {c}"))
                })
                .collect::<Vec<_>>()
                .join("\n");
            (Some(format!("{todos} todos")), Some(list))
        }
        _ => {
            // Any other tool: its first string input, the rest as detail.
            let first = ["command", "file_path", "url", "pattern", "query", "description", "prompt", "task_id"]
                .iter()
                .find_map(|k| s(k))
                .or_else(|| {
                    input
                        .as_object()
                        .into_iter()
                        .flatten()
                        .find_map(|(_, v)| v.as_str().map(str::to_string))
                });
            let detail = match input {
                Value::Object(m) if m.len() > 1 => serde_json::to_string_pretty(input).ok(),
                _ => None,
            };
            (first, detail)
        }
    };
    let headline = headline
        .map(|h| clip(h.lines().next().unwrap_or_default().trim(), HEADLINE_CHARS))
        .unwrap_or_default();
    let keep = match full {
        "Bash" | "Edit" | "Write" | "NotebookEdit" => EDIT_CHARS,
        _ => DETAIL_CHARS,
    };
    let detail = detail.map(|d| clip(&d, keep)).filter(|d| !d.trim().is_empty());
    (name, headline, detail)
}

/// Removed lines with "- ", added lines with "+ ", as the terminal shows an edit.
fn diff(old: &str, new: &str) -> String {
    let mut out = Vec::new();
    for l in old.trim_end().lines() {
        out.push(format!("- {l}"));
    }
    for l in new.trim_end().lines() {
        out.push(format!("+ {l}"));
    }
    out.join("\n")
}

/// The diff Claude Code attaches to a tool result for the files the call changed: a Bash
/// command's `bashEditDiff` (several files, with `moreFiles` left out) or an edit's
/// `structuredPatch` for its `filePath`. The terminal renders this; so does the app.
pub fn changed_files(result: &Value) -> Option<String> {
    let mut out = String::new();
    if let Some(files) = result["bashEditDiff"]["files"].as_array() {
        for f in files {
            push_file(&mut out, f["filePath"].as_str().unwrap_or("?"), &f["hunks"]);
        }
        if let Some(more) = result["bashEditDiff"]["moreFiles"].as_u64().filter(|m| *m > 0) {
            out.push_str(&format!("… {more} more files\n"));
        }
    } else if result["structuredPatch"].is_array() {
        push_file(&mut out, result["filePath"].as_str().unwrap_or("?"), &result["structuredPatch"]);
    }
    let out = out.trim_end();
    (!out.is_empty()).then(|| clip(out, EDIT_CHARS))
}

fn push_file(out: &mut String, path: &str, hunks: &Value) {
    let hunks = hunks.as_array().into_iter().flatten().collect::<Vec<_>>();
    if hunks.is_empty() {
        return;
    }
    out.push_str(&format!("=== {path}\n"));
    for h in hunks {
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            h["oldStart"].as_u64().unwrap_or(0),
            h["oldLines"].as_u64().unwrap_or(0),
            h["newStart"].as_u64().unwrap_or(0),
            h["newLines"].as_u64().unwrap_or(0)
        ));
        for l in h["lines"].as_array().into_iter().flatten() {
            out.push_str(l.as_str().unwrap_or_default());
            out.push('\n');
        }
    }
}

/// The input a person decides on: a command, a path, a URL, a pattern or a question.
pub fn key_input(input: &Value) -> Option<String> {
    ["command", "file_path", "url", "pattern", "description"]
        .iter()
        .find_map(|k| input[k].as_str())
        .map(str::to_string)
        .or_else(|| {
            input["questions"][0]["question"]
                .as_str()
                .map(str::to_string)
        })
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// A tool result's text; images and tool definitions are named, not shown.
fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| match p["type"].as_str() {
                Some("text") => p["text"].as_str().map(str::to_string),
                Some("image") => Some("[image]".to_string()),
                Some("tool_reference") => p["tool_name"].as_str().map(|n| format!("[tool {n} loaded]")),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(source: EventSource, kind: &str, payload: Value) -> Event {
        Event {
            seq: 7,
            time_ms: 1,
            source: source.into(),
            kind: kind.into(),
            payload_json: payload.to_string(),
            ..Default::default()
        }
    }

    fn user(content: Value) -> Event {
        event(EventSource::Transcript, "user", serde_json::json!({"message": {"role": "user", "content": content}}))
    }

    fn shown(e: Event) -> Vec<(Role, String)> {
        items(&e).into_iter().map(|i| (i.role, i.text)).collect()
    }

    #[test]
    fn an_item_is_timed_by_when_claude_code_wrote_it() {
        let mut e = event(
            EventSource::Transcript,
            "assistant",
            serde_json::json!({"timestamp": "2026-10-25T10:59:10.637Z", "message": {"content": [{"type": "text", "text": "hi"}]}}),
        );
        e.time_ms = 9;
        assert_eq!(items(&e)[0].time_ms, 1_792_925_950_637);
        // A line without one keeps the time the node read it.
        let mut e = event(EventSource::Transcript, "assistant", serde_json::json!({"message": {"content": [{"type": "text", "text": "hi"}]}}));
        e.time_ms = 9;
        assert_eq!(items(&e)[0].time_ms, 9);
    }

    #[test]
    fn transcript_lines_become_a_conversation() {
        assert_eq!(shown(user("run the tests".into())), [(Role::User, "run the tests".into())]);
        let assistant = event(
            EventSource::Transcript,
            "assistant",
            serde_json::json!({"message": {"content": [
                {"type": "thinking", "thinking": "hm"},
                {"type": "text", "text": "Running them."},
                {"type": "tool_use", "name": "Bash", "input": {"command": "cargo test", "description": "Run the suite"}}]}}),
        );
        let items = items(&assistant);
        assert_eq!(items[0].role, Role::Thinking);
        assert_eq!(items[1].text, "Running them.");
        assert_eq!((items[2].role.clone(), items[2].tool.as_str(), items[2].text.as_str()), (Role::Tool, "Bash", "cargo test"));
        assert_eq!(items[2].detail.as_deref(), Some("Run the suite"));
        let heredoc = tool_use(&serde_json::json!({"name": "Bash", "input": {"command": "cat > a.txt <<'EOF'\nalpha\nbeta\nEOF", "description": "Write a.txt"}}));
        assert_eq!(heredoc.1, "cat > a.txt <<'EOF'");
        assert_eq!(heredoc.2.as_deref(), Some("cat > a.txt <<'EOF'\nalpha\nbeta\nEOF"));
        // Edits and scripts are kept whole well past other details, for the full-screen view.
        let long = "line\n".repeat(2000);
        let script = tool_use(&serde_json::json!({"name": "Bash", "input": {"command": format!("cat > a <<'EOF'\n{long}EOF")}}));
        assert!(script.2.unwrap().ends_with("line\nEOF"));
        let write = tool_use(&serde_json::json!({"name": "Write", "input": {"file_path": "a", "content": long}}));
        assert_eq!(write.2.unwrap().lines().count(), 2000);
        let other = tool_use(&serde_json::json!({"name": "Agent", "input": {"description": "d", "prompt": long}}));
        assert_eq!(other.2.unwrap().chars().count(), DETAIL_CHARS + 1);
        let result = user(serde_json::json!([{"type": "tool_result", "content": "x".repeat(5000), "is_error": true}]));
        let r = super::items(&result);
        assert_eq!(r[0].role, Role::ToolResult);
        assert!(r[0].error);
        assert_eq!(r[0].text.chars().count(), RESULT_CHARS + 1);
    }

    #[test]
    fn a_result_carries_the_diff_of_the_files_the_call_changed() {
        let bash = event(
            EventSource::Transcript,
            "user",
            serde_json::json!({"message": {"role": "user", "content": [{"type": "tool_result", "content": "ok"}]},
                "toolUseResult": {"stdout": "ok", "bashEditDiff": {"changedFiles": ["/w/a.rs"], "moreFiles": 1,
                    "files": [{"filePath": "/w/a.rs", "hunks": [{"oldStart": 3, "oldLines": 2, "newStart": 3, "newLines": 2,
                        "lines": [" fn main() {", "-    old();", "+    new();"]}]}]}}}),
        );
        let r = items(&bash);
        assert_eq!(r[0].diff.as_deref(), Some("=== /w/a.rs\n@@ -3,2 +3,2 @@\n fn main() {\n-    old();\n+    new();\n… 1 more files"));
        let edit = event(
            EventSource::Transcript,
            "user",
            serde_json::json!({"message": {"role": "user", "content": [{"type": "tool_result", "content": "The file has been updated"}]},
                "toolUseResult": {"filePath": "/w/b.nix", "structuredPatch": [{"oldStart": 1, "oldLines": 1, "newStart": 1, "newLines": 1,
                    "lines": ["-x", "+y"]}]}}),
        );
        assert_eq!(items(&edit)[0].diff.as_deref(), Some("=== /w/b.nix\n@@ -1,1 +1,1 @@\n-x\n+y"));
        let plain = event(
            EventSource::Transcript,
            "user",
            serde_json::json!({"message": {"role": "user", "content": [{"type": "tool_result", "content": "ok"}]},
                "toolUseResult": {"stdout": "ok"}}),
        );
        assert!(items(&plain)[0].diff.is_none());
    }

    #[test]
    fn tool_calls_name_what_matters() {
        let call = |name: &str, input: Value| tool_use(&serde_json::json!({"name": name, "input": input}));
        assert_eq!(
            call("Edit", serde_json::json!({"file_path": "a.rs", "old_string": "x\nz", "new_string": "y"})),
            ("Edit".into(), "a.rs".into(), Some("- x\n- z\n+ y".into()))
        );
        assert_eq!(call("WebSearch", serde_json::json!({"query": "tsnet android", "mode": "standard"})).1, "tsnet android");
        assert_eq!(call("Grep", serde_json::json!({"pattern": "fn main", "path": "src"})).1, "fn main in src");
        let mcp = call("mcp__plugin_x_playwright__browser_navigate", serde_json::json!({"url": "http://h"}));
        assert_eq!((mcp.0.as_str(), mcp.1.as_str()), ("browser_navigate", "http://h"));
        let todo = call("TodoWrite", serde_json::json!({"todos": [{"content": "a", "status": "completed"}, {"content": "b", "status": "pending"}]}));
        assert_eq!((todo.1.as_str(), todo.2.as_deref()), ("2 todos", Some("[x] a\n[ ] b")));
        let unknown = call("Frob", serde_json::json!({"n": 3, "what": "thing"}));
        assert_eq!(unknown.1, "thing");
        assert!(unknown.2.is_some());
    }

    #[test]
    fn what_claude_code_writes_into_user_lines_is_made_readable() {
        let peer = "Another Claude session sent a message:\nReply with just the word osprey.\n\nThis came from another Claude session \u{2014} not typed by your user.";
        assert_eq!(shown(user(peer.into())), [(Role::User, "Reply with just the word osprey.".into())]);
        let note = "<task-notification>\n<task-id>a0</task-id>\n<status>completed</status>\n<summary>Agent \"Verify hooks\" finished</summary>\n<result>…</result>\n</task-notification>";
        assert_eq!(shown(user(note.into())), [(Role::System, "Background task: Agent \"Verify hooks\" finished".into())]);
        let cmd = "<command-name>/model</command-name>\n            <command-message>model</command-message>\n            <command-args>haiku</command-args>";
        assert_eq!(shown(user(cmd.into())), [(Role::System, "/model haiku".into())]);
        let prompted = "<command-message>yajna:project</command-message>\n<command-name>/yajna:project</command-name>\n<command-args>green tea steeping</command-args>";
        assert_eq!(shown(user(prompted.into())), [(Role::User, "/yajna:project green tea steeping".into())]);
        assert!(shown(user("<local-command-stdout>(no content)</local-command-stdout>".into())).is_empty());
        assert_eq!(
            shown(user("<local-command-stdout>\u{1b}[2mCompacted\u{1b}[22m</local-command-stdout>".into())),
            [(Role::ToolResult, "Compacted".into())]
        );
        assert!(shown(user("<local-command-caveat>The command below…</local-command-caveat>".into())).is_empty());
        assert_eq!(shown(user("<bash-input>pwd</bash-input>".into())), [(Role::User, "! pwd".into())]);
        assert_eq!(
            shown(user("<bash-stdout>/tmp</bash-stdout><bash-stderr></bash-stderr>".into())),
            [(Role::ToolResult, "/tmp".into())]
        );
        assert_eq!(
            shown(user(serde_json::json!([{"type": "text", "text": "[Request interrupted by user]"}]))),
            [(Role::System, "Interrupted".into())]
        );
        assert_eq!(
            shown(user("fix it\n<system-reminder>be careful</system-reminder>".into())),
            [(Role::User, "fix it".into())]
        );
        let meta = event(
            EventSource::Transcript,
            "user",
            serde_json::json!({"isMeta": true, "message": {"role": "user", "content": [{"type": "text", "text": "Base directory for this skill: /x"}]}}),
        );
        assert!(shown(meta).is_empty());
        let image = user(serde_json::json!([{"type": "tool_result", "content": [{"type": "image", "source": {"data": "AAAA"}}, {"type": "text", "text": "ok"}]}]));
        assert_eq!(shown(image), [(Role::ToolResult, "[image]\nok".into())]);
    }

    #[test]
    fn turns_get_their_time_and_tokens_from_usage_and_turn_duration() {
        let mut c = Conversation::default();
        let usage = |seq: u64, input: u64, output: u64| Event {
            seq,
            payload_json: serde_json::json!({"claude-haiku-4-5": {"messages": seq, "input_tokens": input,
                "output_tokens": output, "cache_read_input_tokens": 1000, "cache_creation_input_tokens": 0}})
            .to_string(),
            ..event(EventSource::Daemon, "usage", serde_json::json!({}))
        };
        assert!(c.push(&usage(1, 2000, 400)).is_empty());
        let totals = c.take_totals().unwrap();
        assert_eq!((totals[0].input_tokens, totals[0].output_tokens), (2000, 400));
        assert!((totals[0].cost_usd - (2000.0 * 1.0 + 400.0 * 5.0 + 1000.0 * 0.1) / 1e6).abs() < 1e-9);
        assert!(c.take_totals().is_none(), "unchanged since");
        let context = Event {
            payload_json: r#"{"total_tokens":16804,"max_tokens":1000000,"percentage":2.0}"#.into(),
            ..event(EventSource::Daemon, "context", serde_json::json!({}))
        };
        assert!(c.push(&context).is_empty());
        assert_eq!(c.take_context(), Some(ContextUsage { used_tokens: 16804, max_tokens: 1_000_000 }));
        assert_eq!(c.take_context(), None);
        let turn = c.push(&Event {
            seq: 2,
            ..event(EventSource::Transcript, "system", serde_json::json!({"subtype": "turn_duration", "durationMs": 14_200}))
        });
        assert_eq!(turn[0].role, Role::Turn);
        assert_eq!(turn[0].text, "14.2 s · 2.0k in · 400 out · 1.0k cached · $0.004");
        // The next turn counts only what it added.
        c.push(&usage(3, 2500, 450));
        let turn = c.push(&Event {
            seq: 4,
            ..event(EventSource::Transcript, "system", serde_json::json!({"subtype": "turn_duration", "durationMs": 900}))
        });
        assert_eq!(turn[0].text, "900 ms · 500 in · 50 out · $0.001");
    }

    #[test]
    fn the_nodes_queue_report_is_the_whole_queue() {
        let mut c = Conversation::default();
        let report = |items: serde_json::Value| Event {
            payload_json: serde_json::json!({ "items": items }).to_string(),
            ..event(EventSource::Daemon, "queue", serde_json::json!({}))
        };
        assert!(c.push(&report(serde_json::json!([{"id": "q1", "text": "then this", "time_ms": 5}]))).is_empty());
        assert_eq!(c.take_queue(), Some(vec![QueuedPrompt { id: "q1".into(), text: "then this".into(), time_ms: 5 }]));
        assert_eq!(c.take_queue(), None);
        c.push(&report(serde_json::json!([])));
        assert_eq!(c.take_queue(), Some(vec![]));
    }

    #[test]
    fn hooks_and_bookkeeping_show_only_where_they_add_something() {
        let pre = event(EventSource::Hook, "PreToolUse", serde_json::json!({"tool_name": "Bash"}));
        assert!(shown(pre).is_empty());
        let note = event(
            EventSource::Hook,
            "Notification",
            serde_json::json!({"message": "Claude needs your permission to use Bash", "notification_type": "permission_prompt"}),
        );
        assert!(shown(note).is_empty());
        let auth = event(
            EventSource::Hook,
            "Notification",
            serde_json::json!({"message": "Logged in", "notification_type": "auth_success"}),
        );
        assert_eq!(shown(auth)[0].0, Role::System);
        let usage = event(EventSource::Daemon, "usage", serde_json::json!({}));
        assert!(shown(usage).is_empty());
        let interrupted = event(EventSource::Daemon, "interrupted", serde_json::json!({}));
        assert_eq!(shown(interrupted), [(Role::Daemon, "Interrupted".into())]);
        let duration = event(
            EventSource::Transcript,
            "system",
            serde_json::json!({"subtype": "turn_duration", "durationMs": 5}),
        );
        assert!(shown(duration).is_empty());
        let slash = event(
            EventSource::Transcript,
            "system",
            serde_json::json!({"subtype": "local_command", "content": "<command-name>/context</command-name>\n<command-args></command-args>"}),
        );
        assert_eq!(shown(slash), [(Role::System, "/context".into())]);
        let compact = event(EventSource::Transcript, "system", serde_json::json!({"subtype": "compact_boundary", "content": "Conversation compacted"}));
        assert_eq!(shown(compact), [(Role::System, "Conversation compacted".into())]);
        let away = event(EventSource::Transcript, "system", serde_json::json!({"subtype": "away_summary", "content": "Planning…"}));
        assert!(shown(away).is_empty());
    }
}
