//! What the dashboard shows and where the cursor is: the hub's nodes, live sessions, held
//! requests and Claude accounts, kept current from `Watch`, and the mode the keyboard is in.
//! No I/O here; keys become [`Action`]s the event loop carries out.

use std::collections::{BTreeSet, HashMap, HashSet};

use saarthi_proto::control::v1::{ClaudeToken, Node, update};
use saarthi_proto::event::v1::{PendingKind, PendingRequest, Session, SessionStatus};

/// Models offered for a new session: claude's `--model` aliases, empty for its default.
pub const MODELS: [(&str, &str); 5] = [
    ("default", ""),
    ("fable", "fable"),
    ("opus", "opus"),
    ("sonnet", "sonnet"),
    ("haiku", "haiku"),
];

/// Permission modes offered: claude's `--permission-mode` values, empty for the user's.
pub const MODES: [(&str, &str); 4] = [
    ("ask", ""),
    ("accept edits", "acceptEdits"),
    ("plan", "plan"),
    ("bypass", "bypassPermissions"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Sessions,
    Waiting,
}

/// A line of the sessions pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Node(String),
    Session(String, String),
}

/// One question of an AskUserQuestion, as the person answers it.
#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub text: String,
    pub options: Vec<(String, String)>,
    pub multi: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewForm {
    pub node: usize,
    pub cwd: String,
    pub prompt: String,
    pub model: usize,
    pub mode: usize,
    /// 0 the node's default, then each account, then the node's own login.
    pub account: usize,
    pub field: usize,
}

pub const FORM_FIELDS: usize = 6;

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Normal,
    Help,
    /// A line of text for `purpose`.
    Input { purpose: Purpose, text: String },
    /// Kill a session, once confirmed.
    ConfirmKill { node: String, session: String },
    Answer {
        node: String,
        pending: String,
        questions: Vec<Question>,
        at: usize,
        cursor: usize,
        picks: Vec<BTreeSet<usize>>,
    },
    New(NewForm),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Purpose {
    Prompt { node: String, session: String },
    Deny { node: String, pending: String },
}

/// What the event loop is asked to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Quit,
    Reload,
    Allow { node: String, pending: String },
    Deny { node: String, pending: String, message: String },
    Answer { node: String, pending: String, answers: Vec<(String, Vec<String>)> },
    Prompt { node: String, session: String, text: String },
    Interrupt { node: String, session: String },
    Kill { node: String, session: String },
    Spawn {
        node: String,
        cwd: String,
        prompt: String,
        model: String,
        mode: String,
        token: Option<String>,
    },
}

#[derive(Default)]
pub struct State {
    pub hub: String,
    pub linked: bool,
    pub nodes: HashMap<String, Node>,
    pub sessions: HashMap<(String, String), Session>,
    pub pending: Vec<(String, PendingRequest)>,
    pub tokens: Vec<ClaudeToken>,
    pub folded: HashSet<String>,
    pub focus: Option<Pane>,
    pub row: usize,
    pub waiting: usize,
    pub mode: Option<Mode>,
    /// The last outcome, and whether it was an error.
    pub status: Option<(String, bool)>,
}

impl State {
    pub fn new(hub: &str) -> Self {
        Self {
            hub: hub.to_string(),
            focus: Some(Pane::Sessions),
            mode: Some(Mode::Normal),
            ..Default::default()
        }
    }

    pub fn focus(&self) -> Pane {
        self.focus.unwrap_or(Pane::Sessions)
    }

    pub fn mode(&self) -> &Mode {
        self.mode.as_ref().unwrap_or(&Mode::Normal)
    }

    /// Everything at once, after (re)connecting.
    pub fn load(
        &mut self,
        nodes: Vec<Node>,
        sessions: Vec<Session>,
        pending: Vec<(String, PendingRequest)>,
        tokens: Vec<ClaudeToken>,
    ) {
        self.nodes = nodes.into_iter().map(|n| (n.id.clone(), n)).collect();
        self.sessions = sessions
            .into_iter()
            .filter(|s| s.ended_ms.is_none())
            .map(|s| ((s.node_id.clone(), s.id.clone()), s))
            .collect();
        self.pending = pending;
        self.tokens = tokens;
        self.linked = true;
        self.clamp();
    }

    pub fn apply(&mut self, body: update::Body) {
        match body {
            update::Body::Node(n) => {
                self.nodes.insert(n.id.clone(), n);
            }
            update::Body::Session(s) => {
                let key = (s.node_id.clone(), s.id.clone());
                if s.ended_ms.is_some() {
                    self.sessions.remove(&key);
                } else {
                    self.sessions.insert(key, s);
                }
            }
            update::Body::Pending(p) => {
                if let Some(r) = p.request {
                    self.pending.retain(|(n, q)| !(n == &p.node_id && q.id == r.id));
                    self.pending.push((p.node_id, r));
                }
            }
            update::Body::PendingClosed(c) => {
                if let Some(closed) = c.closed {
                    self.pending
                        .retain(|(n, q)| !(n == &c.node_id && q.id == closed.pending_id));
                    // An answer screen for a request that went away has nothing to answer.
                    if let Mode::Answer { node, pending, .. } = self.mode()
                        && node == &c.node_id
                        && pending == &closed.pending_id
                    {
                        self.mode = Some(Mode::Normal);
                        self.say("answered elsewhere", false);
                    }
                }
            }
        }
        self.clamp();
    }

    /// Nodes, connected first then by name.
    pub fn sorted_nodes(&self) -> Vec<&Node> {
        let mut nodes: Vec<&Node> = self.nodes.values().collect();
        nodes.sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.cmp(&b.name)));
        nodes
    }

    /// A node's live sessions, newest first.
    pub fn sessions_of(&self, node: &str) -> Vec<&Session> {
        let mut s: Vec<&Session> = self
            .sessions
            .values()
            .filter(|s| s.node_id == node)
            .collect();
        s.sort_by(|a, b| b.started_ms.cmp(&a.started_ms).then(a.id.cmp(&b.id)));
        s
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for n in self.sorted_nodes() {
            rows.push(Row::Node(n.id.clone()));
            if !self.folded.contains(&n.id) {
                for s in self.sessions_of(&n.id) {
                    rows.push(Row::Session(n.id.clone(), s.id.clone()));
                }
            }
        }
        rows
    }

    /// Held requests, oldest first: the one waiting longest is on top.
    pub fn waiting(&self) -> Vec<&(String, PendingRequest)> {
        let mut w: Vec<&(String, PendingRequest)> = self.pending.iter().collect();
        w.sort_by_key(|(_, p)| p.created_ms);
        w
    }

    pub fn waiting_for(&self, node: &str, session: &str) -> usize {
        self.pending
            .iter()
            .filter(|(n, p)| n == node && p.session_id == session)
            .count()
    }

    fn clamp(&mut self) {
        self.row = self.row.min(self.rows().len().saturating_sub(1));
        self.waiting = self.waiting.min(self.pending.len().saturating_sub(1));
    }

    pub fn say(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some((text.into(), error));
    }

    fn selected_session(&self) -> Option<(String, String)> {
        match self.rows().get(self.row)? {
            Row::Session(n, s) => Some((n.clone(), s.clone())),
            Row::Node(_) => None,
        }
    }

    fn selected_pending(&self) -> Option<(String, PendingRequest)> {
        self.waiting().get(self.waiting).map(|(n, p)| (n.clone(), p.clone()))
    }

    /// Nodes a session can be started on.
    pub fn spawnable(&self) -> Vec<&Node> {
        self.sorted_nodes()
            .into_iter()
            .filter(|n| n.connected && !n.spawn_roots.is_empty())
            .collect()
    }

    /// A key in the current mode; Some when the event loop has work to do.
    pub fn key(&mut self, key: Key) -> Option<Action> {
        let mode = self.mode.take().unwrap_or(Mode::Normal);
        let (mode, action) = match mode {
            Mode::Normal => self.normal(key),
            Mode::Help => (Mode::Normal, None),
            Mode::Input { purpose, text } => input(purpose, text, key),
            Mode::ConfirmKill { node, session } => match key {
                Key::Char('y') => (Mode::Normal, Some(Action::Kill { node, session })),
                _ => (Mode::Normal, None),
            },
            Mode::Answer {
                node,
                pending,
                questions,
                at,
                cursor,
                picks,
            } => answer(node, pending, questions, at, cursor, picks, key),
            Mode::New(form) => self.new_form(form, key),
        };
        self.mode = Some(mode);
        action
    }

    fn normal(&mut self, key: Key) -> (Mode, Option<Action>) {
        let pane = self.focus();
        match key {
            Key::Char('q') | Key::Ctrl('c') => return (Mode::Normal, Some(Action::Quit)),
            Key::Char('?') => return (Mode::Help, None),
            Key::Char('r') => return (Mode::Normal, Some(Action::Reload)),
            Key::Tab => {
                self.focus = Some(match pane {
                    Pane::Sessions => Pane::Waiting,
                    Pane::Waiting => Pane::Sessions,
                });
            }
            Key::Up | Key::Char('k') => match pane {
                Pane::Sessions => self.row = self.row.saturating_sub(1),
                Pane::Waiting => self.waiting = self.waiting.saturating_sub(1),
            },
            Key::Down | Key::Char('j') => match pane {
                Pane::Sessions => self.row += 1,
                Pane::Waiting => self.waiting += 1,
            },
            Key::Char('n') => {
                if self.spawnable().is_empty() {
                    self.say("no connected node allows starting sessions", true);
                } else {
                    return (Mode::New(self.form(0)), None);
                }
            }
            _ => {}
        }
        let action = match pane {
            Pane::Sessions => self.session_key(key),
            Pane::Waiting => self.waiting_key(key),
        };
        self.clamp();
        action
    }

    fn session_key(&mut self, key: Key) -> (Mode, Option<Action>) {
        if let Some(Row::Node(id)) = self.rows().get(self.row).cloned()
            && matches!(key, Key::Enter | Key::Char(' '))
        {
            if !self.folded.remove(&id) {
                self.folded.insert(id);
            }
            return (Mode::Normal, None);
        }
        let Some((node, session)) = self.selected_session() else {
            return (Mode::Normal, None);
        };
        match key {
            Key::Char('p') | Key::Enter => (
                Mode::Input {
                    purpose: Purpose::Prompt { node, session },
                    text: String::new(),
                },
                None,
            ),
            Key::Char('i') => (Mode::Normal, Some(Action::Interrupt { node, session })),
            Key::Char('x') => (Mode::ConfirmKill { node, session }, None),
            _ => (Mode::Normal, None),
        }
    }

    fn waiting_key(&mut self, key: Key) -> (Mode, Option<Action>) {
        let Some((node, p)) = self.selected_pending() else {
            return (Mode::Normal, None);
        };
        let question = p.kind() == PendingKind::Question;
        match key {
            Key::Char('a') if !question => (
                Mode::Normal,
                Some(Action::Allow {
                    node,
                    pending: p.id,
                }),
            ),
            Key::Char('d') => (
                Mode::Input {
                    purpose: Purpose::Deny {
                        node,
                        pending: p.id,
                    },
                    text: String::new(),
                },
                None,
            ),
            Key::Enter if question => {
                let questions = questions(&p.tool_input_json);
                if questions.is_empty() {
                    self.say("the question could not be read; deny it with d", true);
                    return (Mode::Normal, None);
                }
                let picks = vec![BTreeSet::new(); questions.len()];
                (
                    Mode::Answer {
                        node,
                        pending: p.id,
                        questions,
                        at: 0,
                        cursor: 0,
                        picks,
                    },
                    None,
                )
            }
            Key::Enter => (
                Mode::Normal,
                Some(Action::Allow {
                    node,
                    pending: p.id,
                }),
            ),
            _ => (Mode::Normal, None),
        }
    }

    /// A new-session form for the `index`th spawnable node, its first spawn root filled in.
    fn form(&self, index: usize) -> NewForm {
        let nodes = self.spawnable();
        let node = nodes[index.min(nodes.len() - 1)];
        NewForm {
            node: index,
            cwd: node.spawn_roots.first().cloned().unwrap_or_default(),
            prompt: String::new(),
            model: 0,
            mode: 0,
            account: 0,
            field: 1,
        }
    }

    /// The account choices on the form: the node's default, each account, its own login.
    pub fn account_choices(&self, node: &Node) -> Vec<(String, Option<String>)> {
        let default = if node.default_claude_token.is_empty() {
            "node default (own login)".to_string()
        } else {
            format!("node default ({})", node.default_claude_token)
        };
        let mut out = vec![(default, None)];
        out.extend(self.tokens.iter().map(|t| (t.name.clone(), Some(t.name.clone()))));
        if !node.default_claude_token.is_empty() {
            out.push(("own login".into(), Some(String::new())));
        }
        out
    }

    fn new_form(&mut self, mut f: NewForm, key: Key) -> (Mode, Option<Action>) {
        let nodes = self.spawnable();
        if nodes.is_empty() {
            self.say("the node went away", true);
            return (Mode::Normal, None);
        }
        let node = nodes[f.node.min(nodes.len() - 1)].clone();
        let accounts = self.account_choices(&node);
        let cycle = |i: usize, n: usize, forward: bool| {
            if forward { (i + 1) % n } else { (i + n - 1) % n }
        };
        match key {
            Key::Esc => return (Mode::Normal, None),
            Key::Tab | Key::Down => f.field = (f.field + 1) % FORM_FIELDS,
            Key::BackTab | Key::Up => f.field = (f.field + FORM_FIELDS - 1) % FORM_FIELDS,
            Key::Left | Key::Right => {
                let fwd = key == Key::Right;
                match f.field {
                    0 => {
                        let i = cycle(f.node, nodes.len(), fwd);
                        let field = f.field;
                        f = self.form(i);
                        f.field = field;
                    }
                    3 => f.model = cycle(f.model, MODELS.len(), fwd),
                    4 => f.mode = cycle(f.mode, MODES.len(), fwd),
                    5 => f.account = cycle(f.account, accounts.len(), fwd),
                    _ => {}
                }
            }
            Key::Backspace => match f.field {
                1 => drop(f.cwd.pop()),
                2 => drop(f.prompt.pop()),
                _ => {}
            },
            Key::Char(c) => match f.field {
                1 => f.cwd.push(c),
                2 => f.prompt.push(c),
                _ => {}
            },
            Key::Enter => {
                if f.cwd.trim().is_empty() {
                    self.say("a directory is needed", true);
                    return (Mode::New(f), None);
                }
                let token = accounts
                    .get(f.account)
                    .and_then(|(_, t)| t.clone());
                return (
                    Mode::Normal,
                    Some(Action::Spawn {
                        node: node.id.clone(),
                        cwd: f.cwd.trim().to_string(),
                        prompt: f.prompt.trim().to_string(),
                        model: MODELS[f.model].1.to_string(),
                        mode: MODES[f.mode].1.to_string(),
                        token,
                    }),
                );
            }
            _ => {}
        }
        (Mode::New(f), None)
    }
}

fn input(purpose: Purpose, mut text: String, key: Key) -> (Mode, Option<Action>) {
    match key {
        Key::Esc => (Mode::Normal, None),
        Key::Enter => {
            let action = match purpose {
                Purpose::Prompt { node, session } => {
                    if text.trim().is_empty() {
                        return (Mode::Normal, None);
                    }
                    Action::Prompt {
                        node,
                        session,
                        text: text.trim().to_string(),
                    }
                }
                Purpose::Deny { node, pending } => Action::Deny {
                    node,
                    pending,
                    message: text.trim().to_string(),
                },
            };
            (Mode::Normal, Some(action))
        }
        Key::Backspace => {
            text.pop();
            (Mode::Input { purpose, text }, None)
        }
        Key::Char(c) => {
            text.push(c);
            (Mode::Input { purpose, text }, None)
        }
        _ => (Mode::Input { purpose, text }, None),
    }
}

/// Up and down pick an option, space ticks one of a multi-select, Enter takes the
/// highlighted option of a single-select (or the ticked ones) and moves to the next
/// question; after the last it sends every answer.
fn answer(
    node: String,
    pending: String,
    questions: Vec<Question>,
    mut at: usize,
    mut cursor: usize,
    mut picks: Vec<BTreeSet<usize>>,
    key: Key,
) -> (Mode, Option<Action>) {
    let q = &questions[at];
    match key {
        Key::Esc => return (Mode::Normal, None),
        Key::Up | Key::Char('k') => cursor = cursor.saturating_sub(1),
        Key::Down | Key::Char('j') => cursor = (cursor + 1).min(q.options.len().saturating_sub(1)),
        Key::Char(' ') if q.multi => {
            if !picks[at].remove(&cursor) {
                picks[at].insert(cursor);
            }
        }
        Key::Enter => {
            if !q.multi || picks[at].is_empty() {
                picks[at] = BTreeSet::from([cursor]);
            }
            if at + 1 < questions.len() {
                at += 1;
                cursor = 0;
            } else {
                let answers = questions
                    .iter()
                    .zip(&picks)
                    .map(|(q, p)| {
                        let labels = p.iter().filter_map(|i| q.options.get(*i)).map(|o| o.0.clone()).collect();
                        (q.text.clone(), labels)
                    })
                    .collect();
                return (
                    Mode::Normal,
                    Some(Action::Answer {
                        node,
                        pending,
                        answers,
                    }),
                );
            }
        }
        _ => {}
    }
    (
        Mode::Answer {
            node,
            pending,
            questions,
            at,
            cursor,
            picks,
        },
        None,
    )
}

/// The questions of an AskUserQuestion input: `{"questions": [{"question", "options":
/// [{"label", "description"}], "multiSelect"}]}`.
pub fn questions(input_json: &str) -> Vec<Question> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(input_json) else {
        return Vec::new();
    };
    v["questions"]
        .as_array()
        .map(|qs| {
            qs.iter()
                .filter_map(|q| {
                    let options: Vec<(String, String)> = q["options"]
                        .as_array()?
                        .iter()
                        .filter_map(|o| {
                            Some((
                                o["label"].as_str()?.to_string(),
                                o["description"].as_str().unwrap_or_default().to_string(),
                            ))
                        })
                        .collect();
                    (!options.is_empty()).then(|| Question {
                        text: q["question"].as_str().unwrap_or_default().to_string(),
                        options,
                        multi: q["multiSelect"].as_bool().unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One line for a held request: the tool and what it would act on.
pub fn summary(p: &PendingRequest) -> String {
    let v: serde_json::Value = serde_json::from_str(&p.tool_input_json).unwrap_or_default();
    let what = ["command", "file_path", "url", "pattern", "path", "description"]
        .iter()
        .find_map(|k| v[*k].as_str().map(str::to_string))
        .or_else(|| v["questions"][0]["question"].as_str().map(str::to_string))
        .unwrap_or_else(|| p.tool_input_json.clone());
    let what = what.lines().next().unwrap_or_default().to_string();
    let tool = if p.kind() == PendingKind::Question {
        "Question"
    } else {
        p.tool_name.as_str()
    };
    format!("{tool}  {what}")
}

pub fn busy(s: &Session) -> bool {
    s.status() == SessionStatus::Busy
}

/// The keys the dashboard reads, independent of the terminal library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Up,
    Down,
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;
    use saarthi_proto::control::v1::{PendingClosedOnNode, PendingOnNode};
    use saarthi_proto::event::v1::PendingClosed;

    fn node(id: &str, connected: bool) -> Node {
        Node {
            id: id.into(),
            name: format!("{id}-name"),
            connected,
            spawn_roots: vec!["/w".into()],
            ..Default::default()
        }
    }

    fn session(node: &str, id: &str, started_ms: i64) -> Session {
        Session {
            id: id.into(),
            node_id: node.into(),
            cwd: format!("/w/{id}"),
            started_ms,
            ..Default::default()
        }
    }

    fn pending(id: &str, kind: PendingKind, input: &str) -> PendingRequest {
        PendingRequest {
            id: id.into(),
            session_id: "s1".into(),
            kind: kind.into(),
            tool_name: "Bash".into(),
            tool_input_json: input.into(),
            created_ms: 1,
        }
    }

    fn state() -> State {
        let mut s = State::new("http://hub");
        s.load(
            vec![node("a", true), node("b", false)],
            vec![session("a", "s1", 1), session("a", "s2", 2), session("b", "s3", 3)],
            vec![],
            vec![],
        );
        s
    }

    #[test]
    fn rows_list_connected_nodes_first_with_newest_sessions_and_fold() {
        let mut s = state();
        let rows = s.rows();
        assert_eq!(
            rows,
            [
                Row::Node("a".into()),
                Row::Session("a".into(), "s2".into()),
                Row::Session("a".into(), "s1".into()),
                Row::Node("b".into()),
                Row::Session("b".into(), "s3".into()),
            ]
        );
        assert_eq!(s.key(Key::Enter), None);
        assert_eq!(s.rows().len(), 3, "a folded");
        s.key(Key::Char(' '));
        assert_eq!(s.rows().len(), 5, "a unfolded");
    }

    #[test]
    fn updates_add_end_and_close() {
        let mut s = state();
        let mut ended = session("a", "s1", 1);
        ended.ended_ms = Some(5);
        s.apply(update::Body::Session(ended));
        assert!(!s.sessions.contains_key(&("a".into(), "s1".into())));
        s.apply(update::Body::Pending(PendingOnNode {
            node_id: "a".into(),
            request: Some(pending("p1", PendingKind::Permission, r#"{"command":"ls -la\nmore"}"#)),
        }));
        assert_eq!(s.waiting_for("a", "s1"), 1);
        assert_eq!(summary(&s.pending[0].1), "Bash  ls -la");
        s.apply(update::Body::PendingClosed(PendingClosedOnNode {
            node_id: "a".into(),
            closed: Some(PendingClosed {
                pending_id: "p1".into(),
                ..Default::default()
            }),
        }));
        assert!(s.pending.is_empty());
    }

    #[test]
    fn a_session_takes_a_prompt_an_interrupt_and_a_confirmed_kill() {
        let mut s = state();
        s.key(Key::Down);
        assert_eq!(s.key(Key::Char('p')), None);
        for c in "go on".chars() {
            s.key(Key::Char(c));
        }
        assert_eq!(
            s.key(Key::Enter),
            Some(Action::Prompt {
                node: "a".into(),
                session: "s2".into(),
                text: "go on".into()
            })
        );
        assert_eq!(
            s.key(Key::Char('i')),
            Some(Action::Interrupt {
                node: "a".into(),
                session: "s2".into()
            })
        );
        assert_eq!(s.key(Key::Char('x')), None);
        assert_eq!(s.key(Key::Char('n')), None, "anything but y cancels");
        assert_eq!(s.mode(), &Mode::Normal);
        s.key(Key::Char('x'));
        assert_eq!(
            s.key(Key::Char('y')),
            Some(Action::Kill {
                node: "a".into(),
                session: "s2".into()
            })
        );
    }

    #[test]
    fn a_request_is_allowed_denied_with_a_message_or_answered() {
        let mut s = state();
        let q = r#"{"questions":[{"question":"Which?","multiSelect":false,"options":[{"label":"A"},{"label":"B"}]},
                                 {"question":"Also?","multiSelect":true,"options":[{"label":"X"},{"label":"Y"},{"label":"Z"}]}]}"#;
        s.apply(update::Body::Pending(PendingOnNode {
            node_id: "a".into(),
            request: Some(pending("p1", PendingKind::Permission, r#"{"command":"ls"}"#)),
        }));
        s.key(Key::Tab);
        assert_eq!(
            s.key(Key::Char('a')),
            Some(Action::Allow {
                node: "a".into(),
                pending: "p1".into()
            })
        );
        s.key(Key::Char('d'));
        for c in "not that".chars() {
            s.key(Key::Char(c));
        }
        assert_eq!(
            s.key(Key::Enter),
            Some(Action::Deny {
                node: "a".into(),
                pending: "p1".into(),
                message: "not that".into()
            })
        );

        let mut question = pending("p2", PendingKind::Question, q);
        question.created_ms = 0;
        s.apply(update::Body::Pending(PendingOnNode {
            node_id: "a".into(),
            request: Some(question),
        }));
        s.waiting = 0;
        assert_eq!(s.key(Key::Char('a')), None, "a question is answered, not allowed");
        s.key(Key::Enter);
        s.key(Key::Down);
        s.key(Key::Enter); // B
        s.key(Key::Char(' ')); // X
        s.key(Key::Down);
        s.key(Key::Down);
        s.key(Key::Char(' ')); // Z
        assert_eq!(
            s.key(Key::Enter),
            Some(Action::Answer {
                node: "a".into(),
                pending: "p2".into(),
                answers: vec![
                    ("Which?".into(), vec!["B".into()]),
                    ("Also?".into(), vec!["X".into(), "Z".into()]),
                ]
            })
        );
    }

    #[test]
    fn the_new_session_form_starts_on_a_spawnable_node_under_a_chosen_account() {
        let mut s = state();
        s.tokens = vec![ClaudeToken {
            name: "work".into(),
            ..Default::default()
        }];
        s.key(Key::Char('n'));
        let Mode::New(f) = s.mode() else { panic!("{:?}", s.mode()) };
        assert_eq!((f.cwd.as_str(), f.field), ("/w", 1));
        for c in "/x".chars() {
            s.key(Key::Char(c));
        }
        s.key(Key::Tab);
        for c in "fix it".chars() {
            s.key(Key::Char(c));
        }
        s.key(Key::Tab);
        s.key(Key::Right); // fable
        s.key(Key::Right); // opus
        s.key(Key::Tab);
        s.key(Key::Left); // bypass
        s.key(Key::Tab);
        s.key(Key::Right); // work
        assert_eq!(
            s.key(Key::Enter),
            Some(Action::Spawn {
                node: "a".into(),
                cwd: "/w/x".into(),
                prompt: "fix it".into(),
                model: "opus".into(),
                mode: "bypassPermissions".into(),
                token: Some("work".into()),
            })
        );
    }
}
