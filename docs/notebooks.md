# Notebooks, a terminal and voice in the app

The owner's lab notebook is yajna (`github:leoank/yajna`): markdown pages for projects,
experiments, runs, protocols, reviews and a daily log, kept in git, with a CLI and an HTTP
server, `yajna serve`. This document is how the saarthi app becomes the owner's one app for
both the agents and the notebook. The decisions are ADR-0052 to ADR-0055 here and yajna's
ADR-0036; the session that recorded them is [[AGS-20261114-rapbh2]].

## The split

- **yajna owns the notebook and its API.** On each machine one per-user `yajna serve`
  answers on a unix socket for every notebook in the user's yajna registry (yajna's
  ADR-0036). It knows nothing of phones, hubs or tailnets, and yajna keeps working without
  it: markdown and git stay the record.
- **saarthi carries it.** The node lists the service in its config, starts it on demand if
  it is not running, and the hub relays the app's requests to it over the node's existing
  link (ADR-0052). No port, key or process is added to reach a machine's notebooks.
- **The app is the one client.** A notebook section next to the sessions: a web view of
  what `yajna serve` renders, then native screens where the phone needs them (ADR-0053).

```
phone app ──ServiceRequest──▶ hub ──OpenService on Link──▶ node
                              hub ◀──ServiceStream (id)─── node ──unix socket──▶ yajna serve
```

## What the owner asked for, and where it goes

| Feature | Where | Notes |
|---|---|---|
| Start a new project | app, a spawn | A form (node, notebook, kind, brief) spawns `/yajna:project <brief>` in the notebook; its interview comes as questions the app already answers. |
| Look at a project's files | yajna's files viewer over the relay | In a web view first; a native tree and timeline later. |
| Reviews | yajna's review API, app screens | A list of `REV` pages and their threads; a diff in the web view; "address this" prompts the session that made the branch (yajna records its session id on the `AGS` page). |
| A terminal on a node | the node's PTY over the hub (ADR-0054) | Not SSH; off unless the node enables it; drawn with libghostty-vt if it builds for Android ([[EXP-20261114-raqjf0]]). |
| Bench work | yajna's run sheets (its phase 2b), app screens | Steps with their `duration`, `wait`, `check`, `hazard`; ticks into the run's `events.jsonl`; timers and a queue of ticks on the phone, which may be out of reach ([[EXP-20261114-raqkym]]). |
| Push to talk, spoken replies | the app, for every session (ADR-0055) | The bench reuses both for its commands. Recognizer chosen by [[EXP-20261114-raqk9b]]. |

## Order

1. The service relay (ADR-0052) and `yajna serve --socket` over every registered notebook
   (yajna ADR-0036), after [[EXP-20261114-raqjjy]]; the notebook section as a web view.
2. *New project* as a spawn of `/yajna:project`.
3. The terminal, after [[EXP-20261114-raqjf0]].
4. Push to talk and spoken replies, after [[EXP-20261114-raqk9b]].
5. Bench screens, once yajna's run sheets and `events.jsonl` exist, after
   [[EXP-20261114-raqkym]]; `yajna serve` becomes an always-on user service then.

Native review screens and the "address this" loop fit in where reviews ask for them.

## Open questions
- iOS: out of scope for now (ADR-0053).
- Does the hub need to know anything of a service beyond its name, for instance to push a
  notification a service raises ("incubation done")? A later decision, when yajna has
  something to say.
