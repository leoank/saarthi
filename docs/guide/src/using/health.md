# Node health and rate limits

Every node samples its machine every 30 seconds and the hub keeps the samples. Open a
node's health from the chart icon on the home screen, or *Node health* in a session's
menu. It shows one chart per metric over the last hour, 6 hours or 24 hours, refreshed
every minute:

- **CPU**, **Memory** and **Disk** (the disk holding your home directory), in percent;
- **GPU** utilisation, on macOS;
- **Load**, the one-minute load average;
- **Network in** and **Network out**, in MB/s across all interfaces;
- your Claude rate limits: **Claude 5-hour limit**, **Claude 7-day limit** and
  **Claude 7-day Fable limit**, in percent used.

Each chart shows the latest value and the peak over the span. The full list of what a
node reports, including what is not charted, is in [Health metrics](../reference/metrics.md).

The subtitle under the node's name says whether it is online and, when keep-awake is on,
whether it is *keeping the machine awake* (a session is busy) or *letting it sleep*.

The **Keep awake** switch at the top turns that hold on and off while the node runs
(macOS only; `--keep-awake` sets it at start). On, the node keeps the machine out of idle
sleep while any session is busy, so a turn started from the phone finishes when the
screen turns off; lid-close sleep still happens. The switch shows the state the node
last reported, which arrives with its next health sample.

## Rate limits

Claude subscriptions have usage windows: a rolling 5-hour window, a 7-day window, and
weekly limits for some models. saarthi can chart how much of each you have used and
[alert you](../setup/notifications.md#rate-limit-alerts) when one is running out early.

Turn it on per node with `saarthi node serve --claude-usage`. The daemon then reads the
OAuth access token Claude Code stored when you logged in (on macOS the login keychain
item *Claude Code-credentials*, elsewhere `~/.claude/.credentials.json`) and asks
Anthropic's usage endpoint every five minutes, the same numbers `/usage` shows in Claude
Code.

What this does and does not do with your credential:

- It **only reads** the token. It never writes, refreshes or copies it; Claude Code keeps
  refreshing it as usual, and the daemon picks up the new one on its next poll. A copy
  would be dangerous: a refresh in one place revokes the token everywhere else, which
  would log your own Claude Code out.
- It never logs the token or sends it to the hub; only the percentages and reset times
  leave the machine.
- If the endpoint answers *429 Too Many Requests*, the daemon waits as long as it is
  asked to, or doubles its wait up to 30 minutes, and goes back to five minutes after an
  answer. Polling every minute drew 429s for half an hour in testing, so the interval is
  deliberately slow. `--claude-usage-secs` changes it.

On macOS the first read may make the keychain ask whether `security` may read the item;
in testing the item allowed it without a prompt.

Without `--claude-usage`, the node reports the copy Claude Code itself keeps in
`~/.claude.json`, which only changes when you run `/usage`, so it can be hours or days
old. The `usage.age_s` metric says how old.

All the machines logged in to the same Claude account show the same numbers; turning it
on for one of them is enough.

The accounts the hub holds as [named tokens](../setup/hub.md#claude-accounts) have their
own figures, under *Claude accounts* in the app's menu and in `saarthi ctl tokens`. The
usage endpoint refuses those tokens, so they come from the sessions themselves: every turn
of a session started under a token reports its account's windows, and the hub keeps the
latest per account, with its time. With
[usage probes](../setup/hub.md#claude-accounts) on, the hub also runs a tiny turn under
each account about every half hour, so none is more than that old. The alerts go out for those accounts too, titled with
the account's name.
