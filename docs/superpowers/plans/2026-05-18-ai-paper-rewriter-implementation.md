# AI Paper Rewriter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a runnable Tauri 2 + Vue 3 desktop app that parses papers, rewrites paragraphs through an OpenAI-compatible API, compares changes, and exports docx.

**Architecture:** Vue owns upload, progress, settings, comparison, and acceptance state. Rust owns file parsing, API calls, local config, cache, and docx export through Tauri IPC commands.

**Tech Stack:** Tauri 2, Vue 3, Vite, TypeScript, Rust, reqwest, zip, quick-xml, docx-rs, pdf-extract, diff-match-patch.

---

## Tasks

- [ ] Scaffold package, Vite, Tauri, TypeScript, and Rust manifests.
- [ ] Implement Rust models, config store, cache store, parser modules, rewriter, exporter, and IPC commands.
- [ ] Implement Vue pages/components for upload, progress, comparison, and settings.
- [ ] Add diff utility and app store state.
- [ ] Run npm install, typecheck/build, cargo check, and fix compile issues.
- [ ] Start the dev server and report the local URL.
