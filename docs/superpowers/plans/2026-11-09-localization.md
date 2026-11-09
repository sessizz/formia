# Localization Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan inline, task by task.

**Goal:** Turkish/English UI with automatic first-run selection and persisted manual choice.
**Architecture:** Explicit translation calls backed by a shared catalog; static HTML uses data attributes. The settings model stores a validated language code. Language changes rerender UI labels without reloading or stopping scans/previews.
**Tech Stack:** Existing JavaScript modules, Node test runner, Rust/Tauri.
**Spec:** ../specs/2026-11-09-localization.md

## Global Constraints
- No new runtime dependencies; do not touch user files or running app settings during tests.
- Preserve unknown/system technical error details and user-provided names.
- Turkish primary locale selects tr; other primary locales select en; saved tr/en wins.

## Review Focus
- Regional and malformed locales; absent navigator; unsupported saved language.
- Existing settings and favorites without a language field.
- Switching languages during scans, with a preview or settings dialog open.
- Interpolation must not interpret placeholder text in user file names.
- Both catalogs, HTML labels, tooltips, and formatting must remain consistent.

## Tasks
- [x] 1. Write and run failing Node tests for locale resolution, catalog interpolation, and formatting.
- [x] 2. Implement catalog, locale state, settings persistence and selection.
- [x] 3. Translate explicit UI strings and refresh rendered components on language change.
- [x] 4. Run Node integration tests, syntax checks, Rust tests and Windows build; inspect diff.

## Execution record

- First locale tests failed because i18n.js did not exist; legacy-settings tests then failed because language was absent. Both passed after implementation.
- Browser plugin not available; used isolated Playwright with a mocked Tauri bridge and a separately installed test-only dependency outside the repository.
- Final validation: 10 Node tests including browser scenarios passed; 52 Rust tests passed, 3 fixture-dependent tests ignored. JavaScript syntax and diff whitespace checks passed. Windows NSIS installer and portable executable built successfully.
- Independent code review reported no critical or important findings. Follow-up polish keeps preview error, loading, and text truncation labels translatable without changing user content.
- macOS build was not run here. No user settings or open application were modified; no GitHub publication was performed.
