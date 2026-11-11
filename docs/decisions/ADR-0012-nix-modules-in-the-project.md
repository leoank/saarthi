# ADR-0012: The hub and node Nix modules live in the project until they move to neusis

Status: superseded by ADR-0041
Date: 2026-10-16

## Context
Deployment and layout (question 13, the owner's answer in `../architecture-questions.md`).

## Decision
A cargo workspace with crates for proto, node, hub and the client core, an android
directory, and a NixOS module for the hub and a home-manager module for the node inside the
project folder; they move to neusis later.

## Consequences
Step 6 of phase 1 writes the modules here.
