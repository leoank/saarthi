# Notifications

Every notification the hub sends, with the kind `--no-notify` switches off. Setting
them up is in [Notifications](../setup/notifications.md).

| Kind | Sent when | On the phone |
|---|---|---|
| `permission` | a session holds a permission dialog | "laptop: allow Bash?" and the command, file, URL or pattern, with **Allow** and **Deny** |
| `question` | Claude asks a question | "laptop: Claude asks" and the question; tapping opens the session to answer |
| `turn` | a session finishes a turn | "laptop: turn finished" and the session's directory |
| `ended` | a session exits or is killed | "laptop: session ended" and the session's directory |
| `usage` | a rate-limit window is used up, nearly used up or going fast, for a node's own login or a [Claude account](../setup/hub.md#claude-accounts) ("work account: ...") | the window and when it resets; tapping opens the node's health |
| `token` | a Claude token expires within a month, once per expiry date | "Claude token work expires in 30 days" |

To the ntfy topic (`--ntfy`), each is a plain message with a title, a body and a
priority: 5 (urgent) for a window used up; 4 (high) for a held request and the other
usage alerts; 3 (default) for a finished turn and an ended session.

To the app, through UnifiedPush, each is a JSON object the app turns into its own
notification. Its fields are `kind`, `node_id`, `node`, `title` and `body`, plus
`session_id` for anything about a session, `pending_id` and `tool` for a held request,
and `window` and `key` for a usage alert, with `claude_token` for an account's. A `token`
reminder carries `claude_token`, `expires_ms` and `key`.
