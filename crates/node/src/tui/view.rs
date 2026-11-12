//! Drawing the dashboard: sessions by node on the left, what waits for you on the right,
//! the accounts and the keys at the bottom, and a box over it all for the current mode.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};
use saarthi_proto::event::v1::{Launch, Session};

use super::state::{FORM_FIELDS, MODELS, MODES, Mode, Pane, Purpose, Row, State, busy, summary};

/// Terracotta, as in the app.
const ACCENT: Color = Color::Rgb(0xd9, 0x77, 0x57);

pub fn draw(f: &mut Frame, s: &State, now_ms: i64) {
    let [top, main, accounts, keys] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    header(f, top, s);
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(main);
    sessions(f, left, s, now_ms);
    waiting(f, right, s, now_ms);
    f.render_widget(Paragraph::new(accounts_line(s, now_ms)), accounts);
    f.render_widget(Paragraph::new(keys_line(s)).dim(), keys);
    overlay(f, s);
}

fn header(f: &mut Frame, area: Rect, s: &State) {
    let mut spans = vec![Span::styled(" saarthi ", Style::new().fg(ACCENT).bold())];
    spans.push(Span::raw(s.hub.trim_start_matches("http://").to_string()).dim());
    if !s.linked {
        spans.push(Span::raw("  reconnecting…").fg(Color::Yellow));
    }
    if let Some((text, error)) = &s.status {
        spans.push(Span::raw("  "));
        let style = if *error { Style::new().fg(Color::Red) } else { Style::new().fg(Color::Green) };
        spans.push(Span::styled(text.clone(), style));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn block(title: &str, focused: bool) -> Block<'static> {
    let style = if focused { Style::new().fg(ACCENT) } else { Style::new().dim() };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(style)
        .title(Span::styled(format!(" {title} "), style.bold()))
}

fn sessions(f: &mut Frame, area: Rect, s: &State, now_ms: i64) {
    let rows = s.rows();
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            Row::Node(id) => {
                let n = &s.nodes[id];
                let live = s.sessions_of(id).len();
                let fold = if s.folded.contains(id) { "▸" } else { "▾" };
                let dot = if n.connected {
                    Span::raw("●").fg(ACCENT)
                } else {
                    Span::raw("○").dim()
                };
                let mut info = vec![if n.connected { "online".to_string() } else { "offline".to_string() }];
                if live > 0 {
                    info.push(format!("{live} live"));
                }
                if !n.default_claude_token.is_empty() {
                    info.push(format!("account {}", n.default_claude_token));
                }
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{fold} ")).dim(),
                    dot,
                    Span::raw(format!(" {}", n.name)).bold(),
                    Span::raw(format!("  {}", info.join(" · "))).dim(),
                ]))
            }
            Row::Session(node, id) => {
                let sess = &s.sessions[&(node.clone(), id.clone())];
                session_item(sess, s.waiting_for(node, id), now_ms)
            }
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(s.row));
    let focused = s.focus() == Pane::Sessions;
    let list = List::new(items)
        .block(block("Sessions", focused))
        .highlight_style(if focused { Style::new().bg(Color::Rgb(0x3a, 0x38, 0x34)) } else { Style::new() });
    f.render_stateful_widget(list, area, &mut state);
}

fn session_item(sess: &Session, waiting: usize, now_ms: i64) -> ListItem<'static> {
    let (mark, state) = if waiting > 0 {
        (Span::raw("!").fg(Color::Yellow).bold(), "waiting for you".to_string())
    } else if busy(sess) {
        (Span::raw("◐").fg(ACCENT), "working".to_string())
    } else {
        (Span::raw("·").dim(), "idle".to_string())
    };
    let name = sess.cwd.trim_end_matches('/').rsplit('/').next().unwrap_or("session").to_string();
    let parent = home(sess.cwd.trim_end_matches('/').rsplit_once('/').map(|(p, _)| p).unwrap_or(""));
    let launch = match sess.launch() {
        Launch::Wrapper => "terminal",
        Launch::Spawned => "headless",
        _ => "bare",
    };
    let mut info = vec![state, launch.to_string(), ago(sess.started_ms, now_ms)];
    if !sess.claude_token.is_empty() {
        info.push(sess.claude_token.clone());
    }
    ListItem::new(vec![
        Line::from(vec![
            Span::raw("    "),
            mark,
            Span::raw(format!(" {name}")),
            Span::raw(format!("  {parent}")).dim(),
        ]),
        Line::from(Span::raw(format!("      {}", info.join(" · "))).dim()),
    ])
}

fn waiting(f: &mut Frame, area: Rect, s: &State, now_ms: i64) {
    let focused = s.focus() == Pane::Waiting;
    let w = s.waiting();
    if w.is_empty() {
        let p = Paragraph::new("Nothing is waiting for you.")
            .dim()
            .block(block("Waiting", focused));
        f.render_widget(p, area);
        return;
    }
    let items: Vec<ListItem> = w
        .iter()
        .map(|(node, p)| {
            let n = s.nodes.get(node).map(|n| n.name.as_str()).unwrap_or("node");
            let where_ = s
                .sessions
                .get(&(node.clone(), p.session_id.clone()))
                .map(|x| x.cwd.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string())
                .unwrap_or_default();
            ListItem::new(vec![
                Line::from(Span::raw(summary(p)).fg(Color::Yellow)),
                Line::from(Span::raw(format!("  {n} / {where_} · {}", ago(p.created_ms, now_ms))).dim()),
            ])
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(s.waiting));
    let list = List::new(items)
        .block(block(&format!("Waiting ({})", w.len()), focused))
        .highlight_style(if focused { Style::new().bg(Color::Rgb(0x3a, 0x38, 0x34)) } else { Style::new() });
    f.render_stateful_widget(list, area, &mut state);
}

fn accounts_line(s: &State, now_ms: i64) -> Line<'static> {
    if s.tokens.is_empty() {
        return Line::from(Span::raw(" No Claude accounts on the hub").dim());
    }
    let mut spans = vec![Span::raw(" ")];
    for (i, t) in s.tokens.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::raw(t.name.clone()).bold());
        let pct = |p: Option<f64>| p.map(|p| format!("{p:.0}%")).unwrap_or_else(|| "–".into());
        if t.usage_ms > 0 {
            spans.push(Span::raw(format!(
                " 5h {} · 7d {}",
                pct(t.five_hour_pct),
                pct(t.seven_day_pct)
            )));
        }
        let days = (t.expires_ms - now_ms) / 86_400_000;
        let left = Span::raw(format!(" · {days}d left"));
        spans.push(if days < 30 { left.fg(Color::Red) } else { left.dim() });
    }
    Line::from(spans)
}

fn keys_line(s: &State) -> String {
    let keys = match s.mode() {
        Mode::Normal => match s.focus() {
            Pane::Sessions => " ↑↓ move  ⏎/space fold node  p prompt  i interrupt  x kill  n new  tab waiting  ? help  q quit",
            Pane::Waiting => " ↑↓ move  a/⏎ allow  d deny  ⏎ answer a question  n new  tab sessions  ? help  q quit",
        },
        Mode::Input { .. } => " type, ⏎ send, esc cancel",
        Mode::ConfirmKill { .. } => " y kill, anything else cancels",
        Mode::Answer { .. } => " ↑↓ pick  space tick (multi)  ⏎ next / send  esc cancel",
        Mode::New(_) => " tab/↑↓ field  ←→ change  type to edit  ⏎ start  esc cancel",
        Mode::Help => " any key closes",
    };
    keys.to_string()
}

fn overlay(f: &mut Frame, s: &State) {
    match s.mode() {
        Mode::Normal => {}
        Mode::Help => {
            let text = vec![
                Line::from("Sessions pane").bold(),
                Line::from("  ↑↓ / j k   move; ⏎ or space on a node folds its sessions"),
                Line::from("  p or ⏎     send a prompt to the session"),
                Line::from("  i          interrupt its turn (a terminal session needs the wrapper)"),
                Line::from("  x          end it (asks first)"),
                Line::from(""),
                Line::from("Waiting pane").bold(),
                Line::from("  a or ⏎     allow a permission request"),
                Line::from("  d          deny it, with a message Claude sees"),
                Line::from("  ⏎          answer a question"),
                Line::from(""),
                Line::from("Anywhere").bold(),
                Line::from("  n new session · tab switch pane · r reload · q quit"),
            ];
            popup(f, "Keys", text, 70, 16);
        }
        Mode::Input { purpose, text } => {
            let title = match purpose {
                Purpose::Prompt { .. } => "Prompt",
                Purpose::Deny { .. } => "Deny, with a message for Claude (optional)",
            };
            popup(f, title, vec![Line::from(format!("{text}▏"))], 70, 3);
        }
        Mode::ConfirmKill { node, session } => {
            let name = s
                .sessions
                .get(&(node.clone(), session.clone()))
                .map(|x| x.cwd.clone())
                .unwrap_or_default();
            popup(
                f,
                "End this session?",
                vec![Line::from(home(&name)), Line::from(""), Line::from("y ends it; any other key keeps it").dim()],
                60,
                5,
            );
        }
        Mode::Answer {
            questions,
            at,
            cursor,
            picks,
            ..
        } => {
            let q = &questions[*at];
            let mut lines = vec![
                Line::from(format!("{} ({} of {})", q.text, at + 1, questions.len())).bold(),
                Line::from(""),
            ];
            for (i, (label, desc)) in q.options.iter().enumerate() {
                let tick = if q.multi {
                    if picks[*at].contains(&i) { "[x] " } else { "[ ] " }
                } else {
                    ""
                };
                let style = if i == *cursor { Style::new().fg(ACCENT).bold() } else { Style::new() };
                let pointer = if i == *cursor { "› " } else { "  " };
                lines.push(Line::from(vec![
                    Span::styled(format!("{pointer}{tick}{label}"), style),
                    Span::raw(if desc.is_empty() { String::new() } else { format!("  {desc}") }).dim(),
                ]));
            }
            let height = lines.len() as u16 + 2;
            popup(f, "Question", lines, 80, height);
        }
        Mode::New(form) => {
            let nodes = s.spawnable();
            let Some(node) = nodes.get(form.node.min(nodes.len().saturating_sub(1))) else {
                return;
            };
            let accounts = s.account_choices(node);
            let account = accounts.get(form.account).map(|a| a.0.clone()).unwrap_or_default();
            let fields: [(&str, String, bool); FORM_FIELDS] = [
                ("Node", node.name.clone(), true),
                ("Directory", form.cwd.clone(), false),
                ("Prompt", if form.prompt.is_empty() && form.field != 2 { "(none: starts idle)".into() } else { form.prompt.clone() }, false),
                ("Model", MODELS[form.model].0.to_string(), true),
                ("Permissions", MODES[form.mode].0.to_string(), true),
                ("Account", account, true),
            ];
            let mut lines = Vec::new();
            for (i, (label, value, choice)) in fields.into_iter().enumerate() {
                let on = i == form.field;
                let value = match (on, choice) {
                    (true, true) => format!("‹ {value} ›"),
                    (true, false) => format!("{value}▏"),
                    _ => value,
                };
                let style = if on { Style::new().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::new() };
                lines.push(Line::from(vec![
                    Span::raw(format!("{label:>12}  ")).dim(),
                    Span::styled(value, style),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(format!("Under {}", node.spawn_roots.iter().map(|r| home(r)).collect::<Vec<_>>().join(" or "))).dim());
            if MODES[form.mode].1 == "bypassPermissions" {
                lines.push(Line::from("Every tool runs without asking.").fg(Color::Red));
            }
            let height = lines.len() as u16 + 2;
            popup(f, "New session", lines, 80, height);
        }
    }
}

fn popup(f: &mut Frame, title: &str, lines: Vec<Line>, width: u16, height: u16) {
    let area = f.area();
    let w = width.min(area.width.saturating_sub(4));
    let h = height.min(area.height.saturating_sub(2));
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 3,
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(block(title, true));
    f.render_widget(p, rect);
}

/// The home directory as `~`.
fn home(path: &str) -> String {
    let mut parts = path.splitn(4, '/');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(""), Some("Users" | "home"), Some(_), rest) => format!("~/{}", rest.unwrap_or("")).trim_end_matches('/').to_string(),
        _ => path.to_string(),
    }
}

fn ago(ms: i64, now_ms: i64) -> String {
    let mins = (now_ms - ms).max(0) / 60_000;
    match mins {
        0 => "just now".into(),
        m if m < 60 => format!("{m} min ago"),
        m if m < 48 * 60 => format!("{} h ago", m / 60),
        m => format!("{} d ago", m / (24 * 60)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tui::state::{Key, State};
    use saarthi_proto::control::v1::{ClaudeToken, Node, PendingOnNode, update};
    use saarthi_proto::event::v1::{PendingKind, PendingRequest, Session, SessionStatus};

    fn text(s: &State, w: u16, h: u16) -> String {
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, s, 10 * 60_000)).unwrap();
        let buf = t.backend().buffer().clone();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn sample() -> State {
        let mut s = State::new("http://ghost001:7700");
        let node = |id: &str, name: &str, on: bool| Node {
            id: id.into(),
            name: name.into(),
            connected: on,
            spawn_roots: vec!["/home/ank/workspace".into()],
            default_claude_token: if on { "work".into() } else { String::new() },
            ..Default::default()
        };
        let sess = |n: &str, id: &str, cwd: &str, busy: bool| Session {
            id: id.into(),
            node_id: n.into(),
            cwd: cwd.into(),
            started_ms: 0,
            status: if busy { SessionStatus::Busy } else { SessionStatus::Idle }.into(),
            launch: saarthi_proto::event::v1::Launch::Wrapper.into(),
            claude_token: "work".into(),
            ..Default::default()
        };
        s.load(
            vec![node("g", "ghost001", true), node("r", "rogue", true), node("c", "curiosity", false)],
            vec![
                sess("g", "s1", "/home/ank/workspace/saarthi", true),
                sess("r", "s2", "/Users/ank/workspace/hub/leoank/neusis", false),
            ],
            vec![],
            vec![ClaudeToken {
                name: "work".into(),
                usage_ms: 1,
                five_hour_pct: Some(3.0),
                seven_day_pct: Some(37.0),
                expires_ms: 400 * 86_400_000,
                ..Default::default()
            }],
        );
        s.apply(update::Body::Pending(PendingOnNode {
            node_id: "r".into(),
            request: Some(PendingRequest {
                id: "p1".into(),
                session_id: "s2".into(),
                kind: PendingKind::Permission.into(),
                tool_name: "Bash".into(),
                tool_input_json: r#"{"command":"nix flake check"}"#.into(),
                created_ms: 0,
            }),
        }));
        s
    }

    #[test]
    fn the_dashboard_shows_nodes_sessions_requests_and_accounts() {
        let mut s = sample();
        let screen = text(&s, 110, 18);
        for want in [
            "ghost001", "rogue", "curiosity", "saarthi", "~/workspace/hub/leoank", "working",
            "waiting for you", "Bash  nix flake check", "work 5h 3% · 7d 37%", "Waiting (1)",
        ] {
            assert!(screen.contains(want), "{want:?} missing from\n{screen}");
        }
        s.key(Key::Char('n'));
        let form = text(&s, 110, 24);
        assert!(form.contains("New session") && form.contains("Node  ghost001") && form.contains("/home/ank/workspace▏") && form.contains("node default (work)"), "{form}");
    }

    #[test]
    fn paths_and_ages_read_short() {
        assert_eq!(home("/Users/me/workspace/x"), "~/workspace/x");
        assert_eq!(home("/home/me"), "~");
        assert_eq!(home("/srv/x"), "/srv/x");
        assert_eq!(ago(0, 30_000), "just now");
        assert_eq!(ago(0, 5 * 60_000), "5 min ago");
        assert_eq!(ago(0, 3 * 3_600_000), "3 h ago");
        assert_eq!(ago(0, 3 * 86_400_000), "3 d ago");
    }
}
