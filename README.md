# rust-coreconf

[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)

A Rust workspace implementing **CORECONF** (CoAP Management Interface) per
[draft-ietf-core-comi](https://datatracker.ietf.org/doc/draft-ietf-core-comi/),
enabling efficient management of YANG data models over constrained networks using
CoAP and CBOR encoding.

## Workspace

| Crate | Purpose |
|---|---|
| `coreconf-model` | SID file parsing, composite multi-module models, JSON↔CBOR codec, YANG types, instance identifiers |
| `coreconf-runtime` | Predicate-path datastore editing, in-memory and file-backed backends, CORECONF request handling, CoAP transport, observer tracking, operation dispatch |
| `coreconf-cli` | Operator CLI: batch convert, validation, file-backed shell, remote live sessions |

## Quick Start

```rust
use coreconf_model::CoreconfModel;
use coreconf_runtime::Datastore;

// Load a YANG SID file (the weather-station model used in tests)
let model = CoreconfModel::new("tests/fixtures/coreconf-m2m@2026-03-29.sid")?;

let mut ds = Datastore::new_in_memory(model.composite_model().clone());

// Set a leaf inside a keyed list entry
ds.set_path(
    "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit",
    serde_json::json!("W/m2"),
)?;

let value = ds.get_path(
    "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit",
)?;
// => Some("W/m2")

// Enumerate list entries
let preds = ds.predicates("/coreconf-m2m:transducers/transducer")?;
// => ["[type='coreconf-m2m:solar-radiation'][id='0']"]
```

### Decode a FETCH response into a datastore

```rust
let cbor_payload: Vec<u8> = /* CoAP FETCH response */;
let ds = Datastore::from_cbor(model, &cbor_payload)?;
// Or for yang-instances+cbor-seq (Accept: 142):
let ds = Datastore::from_cbor_instance_seq(model, &cbor_payload)?;
```

## CLI

```bash
# Install
cargo install --path crates/coreconf-cli

# JSON → CORECONF CBOR
coreconf-cli convert --sid model.sid --input data.json --output data.cbor

# CBOR → JSON
coreconf-cli convert --reverse --sid model.sid --input data.cbor --output data.json

# Validate a SID file
coreconf-cli validate --sid model.sid

# File-backed interactive shell
coreconf-cli shell --sid model.sid --file config.json

# Live remote session over CoAP
coreconf-cli live --sid model.sid --server [::1]:5683
```

For a full walkthrough of every operation with real output, see [tutorial.md](tutorial.md).

## CORECONF Operations

| Operation  | CoAP Method | Description                                            |
| ---------- | ----------- | ------------------------------------------------------ |
| **GET**    | GET         | Retrieve entire datastore or a predicate path as CBOR  |
| **FETCH**  | FETCH       | Selectively retrieve data nodes by SID or instance ID  |
| **iPATCH** | iPATCH      | Modify data nodes (set SID-value or SID-null for delete) |
| **POST**   | POST        | Invoke YANG RPC or action via registered operation bindings |

### Interface routing

CORECONF defines two CoAP interfaces:

| Path | Purpose | Allowed methods |
|---|---|---|
| `/c` | Management — configuration and telemetry data | GET, FETCH, iPATCH, POST |
| `/s` | Streaming — time-series and event notifications | FETCH + Observe |

### CoAP Observe

The streaming interface (`/s`) supports RFC 7641 Observe for push notifications.
The handler tracks registered observers, marks resources dirty on iPATCH, and
provides pending notification sequences.

Query parameters `c=` (all/config/nonconfig) and `d=` (all/trim defaults) are parsed
and acknowledged; filtering is pass-through until multi-datastore support is added.

## CoAP Transport

A reference `coap-lite` adapter is included. Start a server:

```bash
# From the repo root
cargo run -p coreconf-cli -- live --sid crates/coreconf-runtime/tests/fixtures/coreconf-m2m@2026-03-29.sid --server [::1]:5683
```

Real devices bring their own CoAP stack and implement the `CoreconfClient` trait.

## SID File Format

The parser accepts both raw and RFC 9595 envelope formats:

```json
// Raw (non-enveloped)
{"module-name": "example", "module-revision": "…", "item": […], "key-mapping": {}}

// RFC 9595 enveloped (as emitted by pyang)
{"ietf-sid-file:sid-file": {"module-name": "example", …}}
```

SID values may be integers or strings.  The `items` alias for `item` is also accepted.

## Architecture

```
crates/
  coreconf-model/src/
    sid_file.rs        # SID file parser (RFC 9595 envelope, string SIDs, identity namespace)
    composite_model.rs # Merged multi-module model with collision detection
    types.rs           # 18 YANG types incl. identityref, enumeration, union
    codec.rs           # JSON↔CBOR conversion with SID delta encoding
    instance_id.rs     # Instance identifier encoding/decoding (RFC 9595)

  coreconf-runtime/src/
    datastore.rs       # Predicate-path get/set/delete, from_cbor, from_cbor_instance_seq, resolve_xpath
    path.rs            # PredicatePath parser
    backend.rs         # Backend trait (read_tree / replace_tree)
    memory_backend.rs  # In-memory backend
    file_backend.rs    # File-backed backend (JSON/CBOR with atomic writes)
    request_handler.rs # GET/FETCH/iPATCH/POST dispatch, /c vs /s routing, observer lifecycle
    operations.rs      # OperationBinding trait + OperationRegistry
    coap_types.rs      # Library-agnostic CoAP types: Request, Response, Interface, Observe
    transport/
      coap_lite.rs     # Reference coap-lite adapter (server + client)

  coreconf-cli/src/
    cli.rs             # Clap CLI definition
    session.rs         # Session, FileSession, LiveSession, diff_trees
    commands/
      convert.rs       # JSON↔CBOR batch conversion
      validate.rs      # SID + data validation
      shell.rs         # File-backed interactive shell
      live.rs          # Remote live session over CoAP
```

## Building and Testing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Minimum Supported Rust Version

The MSRV is **Rust 1.85** with edition 2024.

## References

- [CoAP Management Interface (CORECONF)](https://datatracker.ietf.org/doc/draft-ietf-core-comi/) — draft-ietf-core-comi
- [RFC 9595 - CBOR Encoding of YANG Data](https://datatracker.ietf.org/doc/rfc9595/)
- [RFC 7252 - CoAP](https://www.rfc-editor.org/rfc/rfc7252)
- [RFC 8949 - CBOR](https://www.rfc-editor.org/rfc/rfc8949)

## License

GPL-3.0
