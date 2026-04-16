# SVG Forge — Fix Plan

Code review findings organized by severity. Each file contains the problem description, affected code locations, and proposed fixes.

## Files

| # | File | Priority | Issues |
|---|------|----------|--------|
| 01 | [Critical Panics](01-critical-panics.md) | CRITICAL | 5 crash/panic sources from unwraps and division by zero |
| 02 | [Security Issues](02-security-issues.md) | HIGH | Attribute injection, XML context-blind pattern matching |
| 03 | [Logic Errors](03-logic-errors.md) | MEDIUM | Dead code in animation, duplicate names, loop guards |
| 04 | [Data Loss & Races](04-data-loss-races.md) | MEDIUM | Watcher debounce, edit conflicts, whitespace stripping |
| 05 | [Performance & Quality](05-performance-quality.md) | LOW | Undo memory, bbox caching, hand-rolled base64 |

## Recommended fix order

1. **01-critical-panics** — prevents crashes on malformed SVGs
2. **02-security-issues** — prevents attribute injection
3. **03-logic-errors** (3.1 first) — fixes broken animation freeze
4. **04-data-loss-races** (4.2 first) — prevents edit loss during collaboration
5. **05-performance-quality** — improves UX on large files
