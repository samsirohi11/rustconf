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
| `coreconf-runtime` | Predicate-path datastore editing, in-memory and file-backed backends, CORECONF request handling, CoAP transport adapter, operation dispatch |
| `coreconf-cli` | Operator CLI: batch convert, validation, file-backed shell, remote live sessions |
| `rust-coreconf` (root) | Compatibility facade re-exporting the above crates |

## Quick Start

```rust
use rust_coreconf::{CompositeModel, Datastore};

let model = CompositeModel::from_sid_strings(&[r#"{
    "module-name":"example",
    "module-revision":"2026-01-01",
    "item":[
        {"identifier":"example","sid":60000},
        {"identifier":"/example:greeting","sid":60001},
        {"identifier":"/example:greeting/author","sid":60002,"type":"string"},
        {"identifier":"/example:greeting/message","sid":60003,"type":"string"}
    ],
    "key-mapping":{}
}"#])?;

let mut datastore = Datastore::new_in_memory(model);
datastore.set_path("/example:greeting/author", serde_json::json!("Leia"))?;
let value = datastore.get_path("/example:greeting/author")?;
// => Some("Leia")
```

### Predicate-path editing of keyed lists

```rust
// Access list entries with key predicates
datastore.set_path(
    "/example:devices/device[id='rdc-1'][name='sensor']/enabled",
    serde_json::json!(true),
)?;

// Enumerate all entries in a list
let preds = datastore.predicates("/example:devices/device")?;
// => ["[id='rdc-1'][name='sensor']", "[id='rdc-2'][name='actuator']"]

// Delete a single leaf inside a list entry
datastore.delete_path(
    "/example:devices/device[id='rdc-1'][name='sensor']/enabled",
)?;

// Delete an entire list entry
datastore.delete_path(
    "/example:devices/device[id='rdc-1'][name='sensor']",
)?;
```

### File-backed datastore

```rust
use rust_coreconf::{FileBackend, EditableFormat};

let backend = FileBackend::open(model, "config.json", EditableFormat::Json)?;
let mut datastore = Datastore::with_backend(model, backend);

datastore.set_path("/example:config/timeout", serde_json::json!(30))?;
// Changes are staged in memory — save to persist:
// backend.save()?;
```

## CLI

```bash
# Install
cargo install --path crates/coreconf-cli

# JSON → CORECONF CBOR
coreconf-cli convert --sid model.sid --input data.json --output data.cbor

# CBOR → JSON
coreconf-cli convert --reverse --sid model.sid --input data.cbor --output data.json

# Validate SID artifacts and data
coreconf-cli validate --sid model.sid --input data.json

# File-backed interactive shell
coreconf-cli shell --sid model.sid --file config.json
```

Inside the shell:

```
coreconf> set /example:device/enabled true
staged
coreconf> diff
A /example:device/enabled true
coreconf> save
saved config.json
coreconf> quit
```

Supports `get`, `set`, `delete`, `dump`, `diff`, `diff --json`, `save`, `reload`, `quit --discard`, and predicate paths with keyed lists.

### CBOR editable files

```bash
coreconf-cli shell --sid model.sid --file data.cbor
```

Displays values as identifier JSON. Unknown extensions need `--format`:

```bash
coreconf-cli shell --sid model.sid --file data.dat --format json
```

### Live remote session

```bash
coreconf-cli live --sid model.sid --server 192.168.1.50:5683
```

Stages edits locally, detects remote conflicts, and pushes changes via CoAP iPATCH:

```
coreconf-live> get /example:device/enabled
true
coreconf-live> set /example:device/name "my-device"
staged
coreconf-live> diff
M /example:device/name null -> "my-device"
coreconf-live> push
pushed 1 change(s)
coreconf-live> quit
```

## CORECONF Operations

| Operation  | CoAP Method | Description                                            |
| ---------- | ----------- | ------------------------------------------------------ |
| **GET**    | GET         | Retrieve entire datastore or a predicate path as CBOR  |
| **FETCH**  | FETCH       | Selectively retrieve data nodes by SID                 |
| **iPATCH** | iPATCH      | Modify data nodes (set SID-value or SID-null for delete) |
| **POST**   | POST        | Invoke YANG RPC or action via registered operation bindings |

Query parameters `c=` (all/config/nonconfig) and `d=` (all/trim defaults) are parsed
and acknowledged; filtering is pass-through until multi-datastore support is added.

## CoAP Transport

A reference `coap-lite` adapter is included for experimentation:

```bash
# Server
cargo run --example coap_server -- --sid samples/example.sid --data samples/example.json

# Client
cargo run --example coap_client -- --sid samples/example.sid --server 127.0.0.1:5683
```

Real devices should bring their own CoAP stack and implement the `CoreconfClient` trait.

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
    sid_file.rs       # SID file parser (RFC 9595 envelope, string SIDs, identity namespace)
    composite_model.rs # Merged multi-module model with collision detection
    types.rs          # 18 YANG types incl. identityref, enumeration, union
    codec.rs          # JSON↔CBOR conversion with SID delta encoding
    instance_id.rs    # Instance identifier encoding/decoding (RFC 9595)

  coreconf-runtime/src/
    datastore.rs      # Predicate-path get/set/delete with keyed list traversal
    path.rs           # PredicatePath parser
    backend.rs        # Backend trait (read_tree / replace_tree)
    memory_backend.rs # In-memory backend
    file_backend.rs   # File-backed backend (JSON/CBOR with atomic writes)
    request_handler.rs # CORECONF GET/FETCH/iPATCH/POST dispatch
    operations.rs     # OperationBinding trait + OperationRegistry
    coap_types.rs     # Library-agnostic CoAP request/response types
    transport/coap_lite.rs # Reference coap-lite adapter (server + client)

  coreconf-cli/src/
    cli.rs            # Clap CLI definition
    session.rs        # Session, FileSession, LiveSession, diff_trees
    commands/
      convert.rs      # JSON↔CBOR batch conversion
      validate.rs     # SID + data validation
      shell.rs        # File-backed interactive shell
      live.rs         # Remote live session over CoAP
```

## Building and Testing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Or run the full check script:

```bash
bash ci/check.sh
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
