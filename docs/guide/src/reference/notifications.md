# Notifications

Every notification the hub sends, with the kind `--no-notify` switches off. Setting
them up is in [Notifications](../setup/notifications.md).

| Kind | Sent when | On the phone |
|---|---|---|
| `permission` | a session has held a permission dialog for 10 seconds | "laptop: allow Bash?" and the command, file, URL or pattern, with **Allow** and **Deny** |
| `question` | a question has waited 10 seconds for an answer | "laptop: Claude asks" and the question; tapping opens the session to answer |
| `turn` | a session finishes a turn that ran a minute or more | "laptop: turn finished" and the session's directory |
| `ended` | a session exits or is killed | "laptop: session ended" and the session's directory |
| `usage` | a rate-limit window is used up, nearly used up or going fast, for a node's own login or a [Claude account](../setup/hub.md#claude-accounts) ("work account: ...") | the window and when it resets; tapping opens the node's health |
| `token` | a Claude token expires within a month, once per expiry date | "Claude token work expires in 30 days" |

Push interrupts only when it should. A held request is alerted only if nobody answered
it in its first 10 seconds, at the terminal or in the app, and a turn only when it ran a
minute or more, timed from the session's last prompt (a turn whose prompt the hub never
saw is always alerted). The app shows nothing while it is in front, since it shows the
same thing itself, and gives the two kinds that matter most their own channels and
sounds: *Needs you* (a permission request or a question) with a rising chime, *Finished*
(a turn or a session) with a falling one. Android's notification settings for saarthi
list both, to silence or change separately.

To the ntfy topic (`--ntfy`), each is a plain message with a title, a body and a
priority: 5 (urgent) for a window used up; 4 (high) for a held request and the other
usage alerts; 3 (default) for a finished turn and an ended session.

To the app, through UnifiedPush, each is a JSON object the app turns into its own
notification. Its fields are `kind`, `node_id`, `node`, `title` and `body`, plus
`session_id` for anything about a session, `pending_id` and `tool` for a held request,
and `window` and `key` for a usage alert, with `claude_token` for an account's. A `token`
reminder carries `claude_token`, `expires_ms` and `key`.
