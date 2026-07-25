---
name: 'POC PR Template'
about: 'This is a template for the datura network poc'
title: '[PR] '
ref: 'main'
labels:
  - poc
---

## What your poc was about

_(1-3 sentences: what proof does this give.)_

## Did you validate and document your code

- [ ] Comments were added as requested, that are clear and understandable.
- [ ] Variable and function names are informative.
- [ ] The code runs successfully and was tested by you

## poc test(s) coverage (explain how the poc functionality testing works)

_(Per CONTRIBUTING.md Section 10: which test(s) specifically prove the core behavior claimed above — not just that `cargo test` passes, but that a test targets the claimed behavior? Name the test function(s).)_

## AI / LLM usage disclosure (Choose one)

- [ ] No AI/LLM tooling was used to produce this code.
- [ ] AI/LLM tooling was used. I have reviewed, tested, and refactored all of its output per [CONTRIBUTING.md](CONTRIBUTING.md#11-ai--llm-usage), and I can explain every line I'm submitting.

## Self-checklist (mirrors CONTRIBUTING.md Section 13)
- [ ] Cargo.toml and Cargo.lock exist in the poc directory
- [ ] `cargo fmt --check` is clean for every crate I touched.
- [ ] `cargo clippy --all-targets -- -D warnings` is clean for every crate I touched.
- [ ] `cargo test` passes for every crate I touched.
- [ ] Public items I added/changed have `///` docs; new crates have a `//!` module doc.
- [ ] I haven't introduced any bloat files per CONTRIBUTING.md(Section 8).
- [ ] If I used AI/LLM tooling, it's disclosed above and reviewed(Section 11).

## Spec section(s) touched (ignore if no spec was relevant)

_(Which section(s) of `spec/specification.md` (or `spec/formal_spec/`) does this PoC implement or demonstrate? Link or name them.)_