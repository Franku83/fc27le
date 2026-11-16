# Security Policy

FC27LE is an experimental alpha project. Security reports are taken seriously,
especially reports about read-only boundary violations, unsafe process access,
unexpected file or network access, and Lua execution limits.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting workflow:

<https://github.com/Franku83/fc27le/security/advisories/new>

Do not open a public issue for a suspected vulnerability. Do not include game
files, metadata XML, saves, memory dumps, credentials or personal data in a
report. A minimal sanitized description and reproduction steps are enough.

This project does not provide a security email address. GitHub private
advisories are the only supported private channel.

## Scope

In scope:

- code that could write to game memory or files despite the read-only policy
- process-inspection permission or path-handling bugs
- unsafe file handling in the GUI, CLI or Lua runtime
- Lua timeout, cancellation or memory-limit failures
- dependency vulnerabilities with a practical impact

Out of scope:

- cheating, anti-cheat, DRM or online-service issues
- problems caused by modified game files or third-party tools
- requests for write, injection, hook or bypass features
- reports that require publishing game data or proprietary assets

## Response

This is a small volunteer project. Reports are handled on a best-effort basis.
Please allow time for triage and avoid public disclosure before a fix or
coordination is agreed.
