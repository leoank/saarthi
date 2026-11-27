# ADR-0081: Authorization with Cedar

Status: accepted
Date: 2026-11-27
Supersedes: ADR-0021 in part, its client token that may do anything

## Context
Phase PHS-20261126-f3e51h lets the owner's apps use the hub, and onboards other people.
Today a client token from `--client-tokens` may make every control call on every node,
session and account (ADR-0021). An app must be held to what it was granted: its own
sessions, the nodes, launch profiles and Claude accounts it may use, no terminal unless
granted. Another person must be held to their own nodes and sessions and what is shared
with them. The owner asked for the rules to be Cedar policies (<https://github.com/cedar-policy>),
editable without a deploy.

EXP-20261126-f3e5a9 wrote the schema and 22 policies for all of this and checked them in
`spikes/16-cedar` with cedar-policy 4.15.0:

- The schema validates in strict mode, and the crate builds on all three of the hub's
  targets.
- 59 cases across the owner, another person, apps, a monitor and nodes decide as intended.
- A decision takes about 14 µs (p50), flat from 100 to 10,000 sessions. Filtering a list
  one session at a time costs 15 ms at 1,000 sessions and 144 ms at 10,000.
- Rebuilding the entities per call costs about 35 times the decision. Keeping them live
  costs 0.07 to 1.75 ms per update.
- A policy set swapped behind a lock applies on the next call.

## Decision
- **Every call is authorized by Cedar in the hub.** The control service builds a request
  for each call before doing anything: the principal, an action named after the call, the
  resource and a context. Cedar allows it or it is refused. A request the schema does not
  admit fails as it is built and is refused the same way. The node service does the same
  for what a node asks of the hub: a Claude token or the Claude API under an account, and
  reporting a session.
- **The schema is the spike's** (`spikes/16-cedar/saarthi.cedarschema`), kept in the
  hub's source and versioned with it:
  - **Principals:** a `Person` in `Role`s (admin, member, monitor). A device `Client` is
    *in* its person, so the phone and `ctl` have its person's rights. An `App` is *not* in
    its person: it has only its `grant` (nodes, profiles, Claude accounts, terminal) and
    what the policies give apps. A `Node` principal belongs to a person, may be shared, and
    may be embedded in an app.
  - **Resources:** `Node`, `Profile`, `ClaudeAccount`, `Session`, `Shell` and `Hub`.
    `Session` carries its owner, `launched_by` and the app that launched it; its labels
    are entity tags.
  - **Actions:** one per control call, grouped as `read`, `operate`, `shell`, `manage` and
    `admin`. A spawn's profile, account and model go in its context.

  Who a principal is, and how the hub knows it from a credential, is phase item 3's ADR.
- **The hub keeps one live entity set.** It is built at start from the database and the
  token files, and updated where a session, client, node, grant or account changes. Calls
  read it without rebuilding it.
- **Lists narrow first, then ask Cedar.** List, watch, search and pending first narrow by
  the hub's own query: the sessions an app launched, or those a person owns or that are on
  their nodes. Only those rows go to Cedar, each as a `ViewSession` (or the list's action)
  request. A principal whose policies allow every read, an admin or a monitor, skips the
  filter. Partial evaluation was not tried and is not needed at the hub's sizes.
- **Grants are attributes, not templates.** An app's grant is a record on its entity, from
  the hub's database. Cedar's policy templates cannot express a spawn's context checks
  (that the profile is the node's and the account is granted).
- **Policies come from two places (phase item 8).**
  - **The bootstrap set** is a file from Nix (or agenix), read at start and read-only to
    the admin page.
  - **Stored policies** are added from the admin page and kept in the hub's database. Each
    is validated against the schema before it is taken.

  The set is swapped behind a lock when either changes and applies on the next call, with
  no restart. A set that fails validation is refused, and the one in force stays.
- **The default set keeps today's behaviour.** The bootstrap policies ship with the hub:
  the spike's policies, starting with the admin's `permit` on everything. Every client in
  `--client-tokens` is a device of the owner, who is the admin, and every node in
  `--node-tokens` is the owner's. So nothing changes for the phone, `ctl` or the nodes
  until a person, an app or a policy is added.
- **A refusal says what was refused (phase item 11).** The call fails with
  `PERMISSION_DENIED`, naming the action and the resource kind. The hub logs the
  principal, action, resource and the deciding policies, and keeps recent refusals for the
  admin page.

## Alternatives considered
- **Checks written in Rust at each call.** There would be no new dependency, and today's
  two kinds of token need little. But every rule (sharing, grants, a label that forbids a
  kill) would be code, and changing one would need a deploy, which the owner ruled out. It
  would also be scattered over some 30 calls.
- **OPA with Rego.** It is mature and general, but runs as a separate Go process or as
  WASM, and Rego policies are not checked against a schema. Cedar is a Rust crate, its
  validator catches a policy that names a wrong attribute, and its semantics are designed
  to be analysed.
- **Entities built from JSON per call.** That needs no state to keep in step, but costs
  about 35 times the decision, on every call.
- **Policy templates per app.** Templates read well for "this app on these nodes", but
  their slots cannot carry the spawn's context checks.
- **Cedar on each node rather than the hub.** Nodes do not see the principal, which only
  the hub authenticates. The hub decides, and a node trusts what reaches it over its link.

## Consequences
- The hub depends on cedar-policy (4.15.0 in the spike), which builds on its three
  targets. Its build time and size are to be noted when it lands.
- Every control call goes through one function, which is the place a call is refused,
  logged and counted. A new call needs an action in the schema before it works.
- The live entity set has to follow every change that touches authorization. A missed
  update decides on stale facts. Tests cover each change point, and a rebuild from the
  database is cheap enough to run at start and on demand.
- Lists depend on the narrowing query matching the policies. A policy that grants more
  than the query fetches shows less than it allows, never more. Admins and monitors see
  everything because they skip the filter.
- Phase items 7 to 11 build on this: the engine, the two policy sources, node principals,
  who answers an app's held requests, and refusals. Item 3's ADR decides identities and how
  credentials are stored.
- Once this ADR is accepted, ADR-0021's token files remain the bootstrap, but a client
  token no longer means everything.
