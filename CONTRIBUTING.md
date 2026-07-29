# Contributing to Datura Network

## ! Important note - this only containts the guidelines, the automation will be added soon

## Table of Contents

- [1. Introduction](#1-introduction)
- [2. Before you start](#2-before-you-start)
- [3. Environment setup](#3-environment-setup)
- [4. Formatting](#4-formatting)
- [5. Linting / checkers](#5-linting--checkers)
- [6. Pre-commit / pre-push hooks](#6-pre-commit--pre-push-hooks)
- [7. Documentation requirements](#7-documentation-requirements)
- [8. File organization - avoid file bloat](#8-file-organization--avoid-file-bloat)
- [9. Testing expectations](#9-testing-expectations)
- [10. Automated testing for PoC correctness](#10-automated-testing-for-poc-correctness)
- [11. AI / LLM usage](#11-ai--llm-usage)
- [12. Good vs. bad practices](#12-good-vs-bad-practices)
- [13. Submitting your work](#13-submitting-your-work)

## 1. Introduction

This document covers the **code-level workflow** only: formatting, linting,
git hooks, documentation, and file-organization conventions for this
repository.

## 2. Before you start

Before writing any code, get familiar with [`spec/specification.md`](spec/specification.md),
which is the living architecture and threat-model reference for the project
(the formal, machine-checkable side of it lives under
[`spec/formal_spec/`](spec/formal_spec/)).

## 3. Environment setup

Install Rust via [rustup](https://rustup.rs). This repo's tooling targets the
**2024 edition**, so make sure you're on a recent stable toolchain
(`rustup update stable`).

There is a one-time, repo-wide setup step to install our git hooks:

```sh
git config core.hooksPath hooks
```

This only needs to be run **once per fresh clone**, not once per PoC. It
points git directly at the tracked [`hooks/`](hooks/) directory, so
`hooks/pre-commit` and `hooks/pre-push` run as your actual git hooks - no
extra crate, dependency, or install step involved. Since this is a local
git config (not tracked by git), each fresh clone needs to run the command
once.

## 4. Formatting

Run `cargo fmt` before committing. The root [`rustfmt.toml`](rustfmt.toml)
applies automatically to every PoC crate - rustfmt walks up parent
directories looking for a config file, so a single repo-root file covers all
of `Poc/*/` without any extra setup.

Do not add per-PoC `rustfmt.toml` files. A stray PoC-local config silently
overrides the repo standard for just that crate, which is exactly the
inconsistency this file exists to prevent.

## 5. Linting / checkers

`cargo clippy --all-targets -- -D warnings` must pass for any crate you
touch. You don't need to run this manually every time - the pre-commit hook
(see below) runs it automatically across the whole repo.

## 6. Pre-commit / pre-push hooks

The one-time setup in section 3 points git at the two hook scripts in
[`hooks/`](hooks/):

- **pre-commit** - runs `cargo fmt --check` and
  `cargo clippy --all-targets -- -D warnings` across every crate in the repo.
- **pre-push** - runs `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, and `cargo test` across every crate in the repo.

Both run against the whole repo on every commit/push, not just touched
crates.

## 7. Documentation requirements

- Every crate's `main.rs` needs a `//!` module-doc comment: 2-4
  sentences describing what the PoC demonstrates, with a link to the
  relevant section of `spec/specification.md` if applicable.
- Every `pub fn`, `struct`, `enum`, and `trait` needs a `///` doc comment.
  Private/internal items are exempt - we're asking for high-level API docs,
  not exhaustive coverage of every line.
- Every PoC should have its own `README.md` covering its purpose, how to
  build/run it, and sample output where that's useful.

## 8. File organization - avoid file bloat

If a file's entire content is a single function, struct, or enum with no
other supporting logic - roughly under ~20-30 lines - merge it into the
module file that most directly owns or uses it, instead of giving it a
standalone file.1

Validate all the code is actually needed and focus on the PoC scope, everything else is less critical to work perfectly

Validate that the poc has its own Cargo.toml and Cargo.lock

## 9. Testing expectations

`cargo test` must pass for any crate you touched - this is enforced
automatically by the pre-push hook. Not every PoC needs extensive tests:
pure demo or benchmark-style PoCs may reasonably have none, and that's
acceptable. Where a PoC has nontrivial invariants (e.g. protocol
encode/decode round-trips, hashring math, cryptographic properties),
property-based tests via [`proptest`](https://docs.rs/proptest) are
encouraged, but not mandated.

## 10. Automated testing for PoC correctness

Beyond general test hygiene (section 9), every PoC must include at least one
automated test that exercises the PoC's **core demonstrated behavior
end-to-end** and asserts it matches what the PoC claims to demonstrate in its
`//!` module doc and the linked section of `spec/specification.md` (required
by section 7).

A reviewer or future contributor should be able to run `cargo test` and get
concrete evidence the PoC does what its docs say it does - not just that it
compiles or individual units pass in isolation.

**Example:** A Kademlia PoC's module doc claims "iterative lookup converges
to target." An integration-style test in the crate exercises a full lookup
sequence and asserts convergence:

```rust
#[test]
fn demonstrates_kademlia_lookup_converges() {
    let network = setup_test_network();
    let target = random_node_id();
    let result = network.iterative_lookup(target);
    assert!(result.converged, "lookup should converge to target");
    assert_eq!(result.target, target);
}
```

This test is the proof that the PoC delivers on its promise. Label tests
clearly (e.g. with a `demonstrates_` prefix) to make intent obvious.

## 11. AI / LLM usage

Using an LLM or other AI tool to help write code is fine, on one condition:
everything it produces gets tested and refactored until it meets the same
bar as hand-written code in this repo. That means it follows the
formatting, linting, docstring, and file-organization rules laid out in
sections 4-8, and you understand and can explain every line you submit.

What's not accepted is unreviewed, unrefactored AI output - "AI slop,"
vibe-coded PRs, whatever you want to call it. Concretely, that's code
pasted straight from a model without the contributor confirming it builds,
passes tests, respects the anti-bloat rule in section 8, and reads like
something a person deliberately designed rather than something that was
merely generated.

Reviewers will not review or accept PRs that show clear signs of
unreviewed AI output. A few tells to watch for:

- The contributor can't explain or justify a piece of code when asked
  about it in review.
- Style is inconsistent within the same PR, as if different sections were
  never reconciled with each other.
- Boilerplate or comments that just restate the obvious, rather than
  explaining intent.
- Missing tests for nontrivial logic.
- File/module structure that ignores section 8's anti-bloat rule.
- Leftover placeholders - `TODO`s, `foo`/`bar`, stub logic, or
  "// implement this" comments left in as if generated but never finished.
- Variables/functions with non-meaningful names (`x`, `data2`, `temp`,
  `doStuff`) instead of names that convey intent.

The tool you used doesn't matter - the bar is the same either way.
Reviewers are judging the output, not how it was produced.

## 12. Good vs. bad practices

This section reinforces standards from earlier sections via concrete examples.

### Naming and clarity

**Bad:**
```rust
fn x(data: Vec<u8>) -> Result<u32> {
    let y = data.len();
    Ok(y as u32)
}
```

**Good:**
```rust
fn peer_id_from_bytes(serialized: Vec<u8>) -> Result<u32> {
    let id_size = serialized.len();
    Ok(id_size as u32)
}
```

Variables like `x`, `y`, `data2`, `temp` are placeholders. Real code explains intent in names. See section 11.

### Doc comments

**Bad:**
```rust
pub struct Node {
    id: u32,
    neighbors: Vec<u32>,
}
```

**Good:**
```rust
/// A node in the distributed hash table.
/// Holds its own unique ID and references to neighboring peers.
pub struct Node {
    id: u32,
    /// List of neighboring peer IDs in the Kademlia ring.
    neighbors: Vec<u32>,
}
```

Every `pub` item needs a `///` doc comment. See section 7.

### File organization

**Bad:** A file `src/hash_ring/errors.rs` containing only:
```rust
#[derive(Debug)]
pub enum RingError {
    InvalidBucket,
}
```

No other code in the file. 15 lines total, living alone in a subdirectory.

**Good:** Merge it into `src/hash_ring.rs`:
```rust
#[derive(Debug)]
pub enum RingError {
    InvalidBucket,
}

// ... other hash_ring code ...
```

Single small items shouldn't get their own files - that adds navigation overhead without clarity. See section 8.

### Unrefactored AI output (bad practice)

**Bad - tells of unreviewed AI:**
```rust
pub fn process_data(d: Vec<u8>) -> Result<String> {
    // TODO: implement error handling
    let x = d.len();
    let y = x * 2; // placeholder logic
    Ok(format!("len: {}", y))
    // FIXME: needs tests
}
```

- Non-meaningful names (`d`, `x`, `y`).
- Leftover placeholders (`TODO`, `FIXME`, stub logic).
- Contributor likely can't explain every line.

**Good - refactored AI output:**
```rust
/// Encodes a peer's serialized data by doubling its logical size.
/// Used in hashring distance calculations.
///
/// # Errors
/// Returns `Err` if serialization fails.
pub fn encode_peer_size(serialized: &[u8]) -> Result<usize> {
    let base_size = serialized.len();
    Ok(base_size.checked_mul(2)?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_encode_doubling() {
        // proptest or concrete case
    }
}
```

- Clear names that convey purpose.
- No leftover `TODO`s or stubs.
- Has a doc comment explaining intent.
- Tested.
- Code is deliberate and explainable.

See section 11 for AI/LLM discipline - everything must be tested, refactored, and something you can justify line-by-line.

## 13. Submitting your work

The branch/PR flow itself happens on our onion Gitea instance, following the
existing instructions in [`README.md`](README.md)'s "How can I contribute?"
section - that process isn't restated here.

Before you open a PR, run through this checklist:

- `cargo fmt --check` is clean for every crate you touched.
- `cargo clippy --all-targets -- -D warnings` is clean for every crate you
  touched.
- `cargo test` passes for every crate you touched.
- Public items you added or changed have `///` docs; new crates have a
  `//!` module doc.
- You haven't introduced any bloat files per section 8.
- If you used AI/LLM tooling, you've reviewed, tested, and refactored the
  output per section 11 - it isn't unreviewed AI output.
