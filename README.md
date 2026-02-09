# rust-coreconf

[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)

A Rust implementation of **CORECONF** (CoAP Management Interface) per [draft-ietf-core-comi](https://datatracker.ietf.org/doc/draft-ietf-core-comi/), enabling efficient management of YANG data models over constrained networks using CoAP and CBOR encoding.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
rust-coreconf = { git = "https://github.com/samsirohi11/rustconf.git" }
```

Set up a CORECONF server with a datastore:

```rust
use rust_coreconf::{CoreconfModel, Datastore, RequestHandler};
use rust_coreconf::coap_types::{Request, Method};

// Load YANG SID file and create model
let model = CoreconfModel::new("ietf-schc@2026-01-12.sid")?;

// Create datastore and request handler
let datastore = Datastore::new(model);
let mut handler = RequestHandler::new(datastore);

// Handle a CORECONF GET request (returns entire datastore as CBOR)
let request = Request::new(Method::Get);
let response = handler.handle(&request);
```

Build FETCH and iPATCH requests from the client side:

```rust
use rust_coreconf::{RequestBuilder, SidFile};

let sid_file = SidFile::from_file("ietf-schc@2026-01-12.sid")?;

// FETCH specific SIDs
let fetch_payload = RequestBuilder::build_fetch(&[2501, 2502, 2503])?;

// iPATCH to set values
let ipatch_payload = RequestBuilder::build_ipatch(
    &[(2501, serde_json::json!(42))],
    &sid_file,
)?;
```

## What is CORECONF?

CORECONF provides a way to manage IoT devices using:

- **YANG data models** for structured configuration and state data
- **CBOR encoding** for compact binary representation (instead of XML/JSON)
- **CoAP transport** for constrained networks (instead of NETCONF/RESTCONF)
- **SID (Schema Item Identifier)** for efficient numeric addressing of YANG nodes

This makes CORECONF ideal for resource-constrained devices where bandwidth and processing power are limited, while maintaining compatibility with standard YANG tooling.

## Features

- **SID File Parsing** -- load and parse YANG SID files in JSON format with bidirectional path/SID mapping
- **JSON to CBOR Conversion** -- transform JSON configuration into compact CBOR with automatic SID delta encoding
- **CBOR to JSON Conversion** -- decode CBOR responses back to JSON for display and debugging
- **Request Handler** -- process incoming CORECONF requests (GET, FETCH, iPATCH, POST)
- **Request Builder** -- construct CORECONF request payloads for client applications
- **Instance Identifiers** -- YANG instance-identifier encoding per RFC 9595

## CORECONF Operations

| Operation  | CoAP Method | Description                                            |
| ---------- | ----------- | ------------------------------------------------------ |
| **GET**    | GET         | Retrieve entire datastore as a CBOR map with SID keys  |
| **FETCH**  | FETCH       | Selectively retrieve specific data nodes by SID        |
| **iPATCH** | iPATCH      | Modify specific data nodes (set SID-value or SID-null) |
| **POST**   | POST        | Invoke YANG RPC or action operations                   |

## Architecture

```
src/
  lib.rs             # Public API re-exports
  coreconf.rs        # CoreconfModel: SID file + JSON/CBOR conversion
  sid.rs             # SidFile: YANG path <-> SID mapping
  datastore.rs       # Datastore: hierarchical YANG data instances
  handler.rs         # RequestHandler: server-side CORECONF protocol
  request_builder.rs # RequestBuilder: client-side CBOR payload construction
  instance_id.rs     # YANG instance-identifier encoding/decoding
  coap_types.rs      # Library-agnostic CoAP request/response types
  types.rs           # YANG node types (containers, lists, leaves)
  error.rs           # Error types (CoreconfError)
```

### Module Responsibilities

| Module               | Purpose                                                                |
| -------------------- | ---------------------------------------------------------------------- |
| `CoreconfModel`      | Central entry point: SID file + JSON/CBOR conversion with delta encoding |
| `SidFile`            | Parses `.sid` files, provides bidirectional path/SID lookups             |
| `Datastore`          | Manages YANG data instances with get/set/delete by path or SID           |
| `RequestHandler`     | Server-side: processes CoAP requests, manipulates datastore              |
| `RequestBuilder`     | Client-side: constructs FETCH and iPATCH CBOR payloads                   |
| `InstancePath`       | Delta-encoded YANG instance-identifiers (RFC 9595)                       |

## SID Files

SID (Schema Item Identifier) files are JSON documents mapping YANG schema nodes to numeric identifiers. They contain:

- **Module name and revision** -- identifies the YANG module
- **Assignment ranges** -- numeric ranges allocated for SIDs
- **Item mappings** -- maps each YANG path to its SID, namespace, and optional type

SIDs enable compact CBOR encoding by replacing verbose YANG paths with small integers. Delta encoding further reduces size by transmitting differences between consecutive SIDs.

## Integration

The library is designed to work with any CoAP implementation. `RequestHandler` accepts abstract request objects and returns abstract responses, decoupling from specific CoAP transport layers. The `coap_types` module defines the library-agnostic interface.

## Examples

The project includes working examples:

```bash
# Run the CORECONF server
cargo run --example coap_server -- --sid model.sid --data initial.json -v

# Run the interactive client
cargo run --example coap_client -- --sid model.sid
```

### Server

Loads a SID file and optional initial data, listens for CoAP requests, processes all CORECONF operations against an in-memory datastore. Supports verbose mode with raw CBOR hex dumps. Saves data on Ctrl+C.

### Client

Interactive REPL for exploring and modifying data. Supports `get`, `fetch`, `set`, and `delete` commands with automatic CBOR-to-JSON conversion. Validates SIDs against the loaded model.

## Building and Testing

```bash
cargo build
cargo test
cargo clippy
```

## Minimum Supported Rust Version

The MSRV is **1.85** (Rust edition 2024).

## References

- [CoAP Management Interface (CORECONF)](https://datatracker.ietf.org/doc/draft-ietf-core-comi/) -- draft-ietf-core-comi
- [RFC 9595 - CBOR Encoding of YANG Data](https://datatracker.ietf.org/doc/rfc9595/)
- [RFC 7252 - CoAP](https://www.rfc-editor.org/rfc/rfc7252)
- [RFC 8949 - CBOR](https://www.rfc-editor.org/rfc/rfc8949)

## License

GPL-3.0
