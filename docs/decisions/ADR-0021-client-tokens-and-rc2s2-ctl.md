# ADR-0021: Clients authenticate with their own token file, and `rc2s2 ctl` stands in for the phone

Status: accepted; superseded in part by ADR-0081
Date: 2026-10-19

## Context
Step 4 needs a client before the phone app exists, and clients must not pass as nodes.

## Decision
The hub reads a separate `--client-tokens` file; `rc2s2 ctl` is a command-line client of
the control service in the node binary.

## Consequences
Live checks and scripts drive intervention without the app. Both token files give way to
the credential store.
