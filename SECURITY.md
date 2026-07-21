# Security Policy

## Supported Versions

Rutile is pre-1.0 (v0.1, MVP stage). Only the latest release is supported —
there are no maintained older branches to backport fixes to.

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Instead, use [GitHub's private vulnerability reporting](https://github.com/yatoub/Rutile/security/advisories/new)
for this repository. This opens a private discussion with the maintainer
before anything becomes public.

If that isn't available to you, open an issue asking for a private contact
channel, without describing the vulnerability itself.

**Response time**: this is a personal open-source project, maintained
outside of working hours — expect an initial response within a week, not
guaranteed within a fixed SLA. Confirmed vulnerabilities will be fixed and
disclosed as a GitHub Security Advisory once a patch is available.

## Scope

Rutile is a desktop terminal emulator (GTK4/libadwaita/vte4) that spawns
and controls shell processes by design — that is expected behavior, not a
vulnerability in itself. Relevant reports include (non-exhaustively):

- Memory safety issues in Rutile's own code (not upstream gtk4-rs/vte4/GTK)
- Privilege escalation or sandbox escape beyond what a normal terminal
  emulator already implies
- Supply-chain concerns about the build/release/packaging pipeline
  (`.github/workflows/`, `PKGBUILD`, `rutile.spec`) — e.g. an unpinned
  dependency or action that could be used to inject malicious code into a
  release artifact
