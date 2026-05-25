# Connections Keyring And MEXC Client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Connections panel manage exchange connection rows and local credentials safely, then add MEXC signed API primitives without wiring live order execution.

**Architecture:** Connections rows persist only non-secret metadata in UI state; API key/secret values are stored via the existing cross-platform `keyring` dependency. The exchange crate gets small, typed MEXC spot/futures signing helpers and private client methods that are callable later from DOM trading code but are not connected to click handling in this pass.

**Tech Stack:** Rust 2024, iced UI, keyring, reqwest, serde, HMAC-SHA256.

---

### Task 1: Credential Metadata And Storage

**Files:**
- Create: `data/src/config/connection_credentials.rs`
- Modify: `data/src/config.rs`
- Test: `data/src/config/connection_credentials.rs`

- [ ] **Step 1: Write failing tests**
  - Add unit tests for stable credential account ids, secret redaction, and round-trip payload serialization.

- [ ] **Step 2: Run focused test**
  - Run: `cargo test -p flowsurface-data config::connection_credentials`
  - Expected: fail before the module exists.

- [ ] **Step 3: Implement keyring-backed credential helpers**
  - Add `ConnectionCredentialRef`, `ConnectionSecret`, `save_connection_secret`, `load_connection_secret`, and `delete_connection_secret`.
  - Keep log messages secret-free.

- [ ] **Step 4: Verify focused test passes**
  - Run: `cargo test -p flowsurface-data config::connection_credentials`

### Task 2: Minimal Connections Panel

**Files:**
- Modify: `src/panel_window.rs`
- Replace focused behavior in: `src/panel_window/connections.rs`

- [ ] **Step 1: Simplify default rows**
  - Default rows are OKX spot view, OKX futures view, MEXC spot view, and MEXC futures view.

- [ ] **Step 2: Add editable draft row**
  - `Add connection` opens one draft row with pick lists for exchange, market, and mode.
  - Trade mode reveals access-key and secret-key inputs; save stores secrets in keyring.

- [ ] **Step 3: Remove color/proxy columns**
  - Table columns become enabled, exchange, market, mode, credentials, status, actions.

- [ ] **Step 4: Mark unsupported exchanges**
  - Bybit displays “Will be implemented soon”; every other non-MEXC exchange displays “Not implemented yet”.

### Task 3: MEXC Signed Client Primitives

**Files:**
- Create: `exchange/src/adapter/hub/mexc/private.rs`
- Modify: `exchange/src/adapter/hub/mexc.rs`
- Modify: `exchange/Cargo.toml`

- [ ] **Step 1: Write failing signing tests**
  - Spot signature test uses the public MEXC example payload.
  - Futures signature test verifies deterministic header signature input.

- [ ] **Step 2: Run focused test**
  - Run: `cargo test -p flowsurface-exchange adapter::hub::mexc::private`
  - Expected: fail before private module exists.

- [ ] **Step 3: Implement private API types and methods**
  - Add spot account/test-order/new-order method builders.
  - Add futures assets/open-positions/open-orders/place-order method builders.
  - Keep methods isolated from DOM click handling.

- [ ] **Step 4: Verify focused test passes**
  - Run: `cargo test -p flowsurface-exchange adapter::hub::mexc::private`

### Task 4: Verification

**Files:**
- All changed Rust files.

- [ ] **Step 1: Format**
  - Run: `cargo fmt --all -- --check`

- [ ] **Step 2: Check compile**
  - Run: `cargo check --workspace`

- [ ] **Step 3: Commit in focused commits**
  - Commit the plan, credential/UI work, and MEXC private client work separately if possible.
