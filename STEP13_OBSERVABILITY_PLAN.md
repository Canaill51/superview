# Step 13 Observability Plan (GUI-Only)

## Goal
Track encoding health and performance for GUI-driven encoding sessions.

## Scope
- Observability hooks in `common/observability.go`.
- Metrics capture from `PerformEncoding` lifecycle.
- GUI access to latest encoding metrics for user-facing reporting.

## Current Design
- Event recorder captures progress, warnings, and failures.
- Metrics include timing, throughput, and output metadata.
- GUI remains decoupled through `UIHandler` and shared pipeline interfaces.

## Planned Checks
```powershell
go test ./common
```

## Outcome
Observability roadmap is aligned with GUI-only execution paths.