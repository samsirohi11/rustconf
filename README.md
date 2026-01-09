# rust-coreconf

A Rust implementation of **CORECONF** (CoAP Management Interface), the protocol that enables efficient management of YANG data models over constrained networks using CoAP and CBOR encoding.

## What is CORECONF?

CORECONF is defined in [CoAP Management Interface (CORECONF)](https://datatracker.ietf.org/doc/draft-ietf-core-comi/) (formerly draft-ietf-core-comi) and provides a way to manage IoT devices and network equipment using YANG data models.

- **YANG data models** for structured configuration and state data
- **CBOR encoding** for compact binary representation (instead of XML/JSON)
- **CoAP transport** for constrained networks (instead of NETCONF/RESTCONF)
- **SID (Schema Item Identifier)** for efficient numeric addressing of YANG nodes

This makes CORECONF ideal for resource-constrained devices where bandwidth and processing power are limited, while maintaining full compatibility with standard YANG tooling and data models.

## Project Overview

This library provides a rust implementation of CORECONF. It was designed as a foundational component for SCHC (Static Context Header Compression) rule management, enabling remote configuration of SCHC rules.

### Key Features

- **SID File Parsing**: Load and parse YANG SID files in JSON format, providing bidirectional mapping between YANG paths and numeric identifiers
- **JSON to CBOR Conversion**: Transform human-readable JSON configuration into compact CBOR with automatic SID delta encoding
- **CBOR to JSON Conversion**: Decode CBOR responses back to JSON for display and debugging
- **Request Handler**: Process incoming CORECONF requests (GET, FETCH, iPATCH, POST) with full datastore support
- **Request Builder**: Construct CORECONF request payloads for client applications
- **Instance Identifiers**: Support for YANG instance-identifier encoding per RFC 9595

## Architecture

The library is organized into several interconnected modules:

### Core Modules

**CoreconfModel** (`coreconf.rs`) serves as the central entry point, combining SID file information with conversion capabilities. It handles the translation between JSON representations and CBOR-encoded CORECONF payloads using delta-SID encoding for efficiency.

**SidFile** (`sid.rs`) parses and stores SID mapping information from `.sid` files. It provides lookups in both directions: from YANG paths to SIDs and from SIDs back to paths. It also tracks type information for leaf nodes.

**Datastore** (`datastore.rs`) manages the actual YANG data instances. It supports hierarchical JSON structures with get, set, and delete operations addressed by either YANG path or SID number.

**RequestHandler** (`handler.rs`) implements the server-side CORECONF protocol logic. It processes incoming requests and manipulates the datastore accordingly, returning properly formatted responses.

**RequestBuilder** (`request_builder.rs`) provides the client-side counterpart, constructing CBOR payloads for FETCH and iPATCH requests that can be sent to a CORECONF server.

### Supporting Modules

**InstancePath** (`instance_id.rs`) handles YANG instance-identifier encoding and decoding, supporting the delta-encoded paths used in CORECONF's yang-instances+cbor-seq format.

**CoAP Types** (`coap_types.rs`) defines library-agnostic request and response structures, allowing the library to work with any CoAP implementation.

**Types** (`types.rs`) provides core data structures for YANG nodes, including containers, lists, and leaf nodes with their associated values.

## CORECONF Operations

The library implements the four main CORECONF operations:

### GET

Retrieves the entire datastore contents as a single CBOR map with SID keys. This is useful for initial synchronization or debugging but transfers more data than necessary for targeted queries.

### FETCH

Selectively retrieves specific data nodes by their SID. The request contains a sequence of SIDs (as CBOR integers), and the response returns only the requested values as a sequence of SID-value pairs. This is the most efficient way to read specific configuration or state data.

### iPATCH (Incremental PATCH)

Modifies specific data nodes without affecting others. The request contains a sequence of SID-value pairs to set, or SID-null pairs to delete. This enables atomic updates to multiple values in a single request.

### POST

Invokes YANG RPC or action operations. The request contains the RPC's SID along with any input parameters, and the response includes any output values.

## Example Applications

The project includes a practical example demonstrating the usage:

### CoAP Server (`examples/coap_server.rs`)

A CORECONF server that:

- Loads a SID file and optional initial data from JSON
- Listens for CoAP requests on an UDP port
- Processes all CORECONF operations against an in-memory datastore
- Supports verbose mode with raw CBOR hex dumps for debugging
- Saves modified data to a JSON file on graceful shutdown (Ctrl+C)

### CoAP Client (`examples/coap_client.rs`)

An interactive REPL-style client for exploring and modifying the data:

- Displays all available SIDs from the loaded model
- Supports get, fetch, set, and delete commands
- Shows human-readable output with automatic CBOR-to-JSON conversion
- Validates SIDs against the model before sending requests

## SID Files

SID (Schema Item Identifier) files are JSON documents that map YANG schema nodes to numeric identifiers.
A SID file contains:

- **Module name and revision**: Identifies the YANG module
- **Assignment ranges**: Defines the numeric ranges allocated for SIDs
- **Item mappings**: Maps each YANG path to its SID, namespace, and optional type

SIDs enable extremely compact CBOR encoding by replacing verbose YANG paths with small integers. The delta encoding further reduces size by transmitting differences between consecutive SIDs rather than absolute values.

## Integration

The library is designed to integrate seamlessly with existing Rust CoAP implementations. The `RequestHandler` accepts abstract request objects and returns abstract responses, allowing you to use any CoAP transport layer (embedded, async, sync).

For embedded systems, the library has minimal dependencies and avoids dynamic allocation where possible. The core conversion functions work with byte slices and can be used in `no_std` environments with minor modifications.

## Building and Testing

The library uses standard Cargo conventions. Run tests with `cargo test` and build examples with `cargo build --examples`. The examples require a CoAP-capable environment (the included examples use UDP sockets with coap-lite).

## License

GPL-3.0
