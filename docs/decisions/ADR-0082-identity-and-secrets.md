# ADR-0082: Identity and secrets

Status: proposed
Date: 2026-11-27
Amends: ADR-0021, ADR-0039

## Context
ADR-0081 authorizes every call with Cedar. Cedar decides about principals; this decision
says who they are, how the hub knows one from a credential, and where the credentials live.

Today the hub reads three agenix files: `--node-tokens` and `--client-tokens`
(`<name> <token>` lines, ADR-0021) and `--claude-tokens` (ADR-0039). A node or client is
the name its token maps to, and the hub already ignores the name a node registers with.
Adding or revoking anything means editing a file and deploying. Phase PHS-20261126-f3e51h
lets the owner add an app's token, onboard a person, and revoke either from an admin page
the hub serves, with the agenix files kept to bootstrap the hub.

The owner chose passkeys for signing in to the admin page (2026-11-27).

## Decision
- **Principals have hub-assigned identities.** A principal's id, name and owner are what
  the hub recorded when it was added, never what it claims:
  - **A person** has a name and roles (admin, member, monitor).
  - **A client** is a device of one person (the phone, `ctl`, the TUI) and acts as them.
  - **An app** belongs to one person and acts only within its grant (ADR-0081).
  - **A node** belongs to one person, or is embedded in one app (ADR-0080). A node's
    registered name is shown beside the hub's, as now.
- **`launched_by` is stamped by the hub.** A session spawned or resumed by a call records
  the principal whose credential made it: a client or an app, by id. A session the node
  started (at its terminal, under `saarthi run`, or by an embedded node's own app) records
  the node. The request has no field for it. It is stored with the session, carried in
  `Session` and kept through a resume. A session's owner is the launcher's person, or the
  node's for a session the node started. An app's own labels on a session (phase item 5)
  are separate, and set by the app.
- **Bearer tokens for clients, apps and nodes, kept as hashes.**
  - A token is 32 random bytes, base64url, with a prefix naming its kind: `sc_`, `sa_` or
    `sn_`.
  - The hub shows a new token once and stores only its SHA-256. A token is random, so a
    slow hash would add nothing.
  - A call is authenticated by looking up the hash of its bearer token, compared in
    constant time.
  - Each token row keeps its principal, creation time, creator, last use (time and
    address or tailnet peer) and, once revoked, when and by whom.
- **Claude tokens added from the admin page are encrypted.** The hub encrypts them with
  XChaCha20-Poly1305 under a 32-byte key read from an agenix file
  (`--secrets-key-file`), each with a fresh nonce and its name as associated data. They are
  decrypted only to relay a request (ADR-0051). Without the key file the hub takes no
  Claude tokens from the page. A Claude account belongs to a person and may be shared with
  others (ADR-0081).
- **The agenix files bootstrap the hub.**
  - At start the hub reads `--client-tokens`, `--node-tokens` and `--claude-tokens` as it
    does now, and gives each line a principal or account:
    - every client is a device of the owner;
    - every node and Claude account is the owner's.
  - The owner is the person named by `--owner` (default `owner`), with the admin role.
  - These entries are listed on the admin page as bootstrap. One can be revoked there: the
    revocation is stored and holds while the line stays in the file. It is replaced only
    by editing the file.
- **Revocation and offboarding.**
  - A revoked token fails its next call. The hub also closes the open streams that token
    authenticated: a watch, or a node's link.
  - Offboarding a person revokes all they hold: their clients' and apps' tokens, their
    nodes' tokens, their passkeys and their admin sessions.
  - Their sessions stay in history under them, and their Claude accounts stop relaying.
- **People sign in to the admin page with passkeys.**
  - An admin makes a one-time enrolment link for a person. It is single-use and expires in
    24 hours. Opening it registers a passkey (WebAuthn, discoverable, user verification
    required) for that person.
  - The first one comes from a device of the bootstrap admin: `saarthi ctl admin enrol
    <person>` prints a link, since that client acts as the owner.
  - Signing in with a passkey gives a session cookie (HttpOnly, Secure, SameSite=Strict),
    backed by a row in the hub's database. It lasts 12 hours, and signing out or
    offboarding ends it.
  - A person may hold several passkeys and remove their own. An admin may remove anyone's.
  - The admin page's calls are the control service's, made as that person, so ADR-0081's
    policies hold for them as for any client.
- **Passkeys need a secure origin.** The admin listener serves HTTPS under a name the
  browser trusts: the hub's tailnet name with the tailnet's certificate, or a certificate
  given by file. Which of these works is checked by EXP-20261127-1gz93p before phase
  item 18 is built.

## Alternatives considered
- **A one-time sign-in token, pasted for a cookie.** Simpler, but it is a password-like
  secret that can be copied around until it is used. The owner chose passkeys.
- **The tailnet identity (WhoIs on the peer).** It needs no secrets at all, but every
  person would have to be on the owner's tailnet, and people would be tied to Tailscale
  accounts.
- **An external identity provider (OIDC).** That means a dependency on a service the hub
  would need to reach, for a handful of people.
- **Tokens hashed with Argon2.** That is meant for low-entropy passwords. A 256-bit random
  token gains nothing from it, and each call would pay for it.
- **Claude tokens in agenix only.** No key is kept at the hub, but adding an account would
  need a deploy, which is what the admin page is for.
- **Encrypting with age under the host's SSH key.** That reuses agenix's mechanism, but
  ties the hub's runtime to the host key. A key of its own, delivered by agenix, can be
  rotated without touching the host.

## Consequences
- The hub's database gains tables for people, principals and their tokens, grants, Claude
  accounts, passkeys, enrolment links and admin sessions. The token files become their
  bootstrap rows. `tokens.rs` gives way to a lookup by hash.
- `launched_by` arrives in the protos and the app (phase items 4 to 6). Sessions from
  before it record none and show as the node's.
- Losing the key file loses the Claude tokens added from the page, but not the agenix
  ones. Its agenix secret is the backup.
- The admin page cannot run on plain HTTP. Until EXP-20261127-1gz93p settles how it gets
  a certificate, item 18 waits on it.
- A person signs in on each browser they use with a passkey. Losing all of them means an
  admin enrols them again.
