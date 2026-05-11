# CORECONF CLI Tutorial

A walkthrough of every operation using the weather-station YANG model
(`coreconf-m2m@2026-03-29`).  This model defines transducers with
identityref-typed keys, enumerations, nested containers, and streaming
data structures — a realistic constrained-device management schema.

All examples assume you're in the repo root.  Demo files live in `tutorial/`:

```
tutorial/
├── coreconf-m2m@2026-03-29.sid   # YANG SID file (98 entries)
├── data.json                      # sample transducer data
└── server_config.json             # for the live demo
```

## 1. Validate a SID file and data

```bash
$ coreconf-cli validate \
    --sid tutorial/coreconf-m2m@2026-03-29.sid \
    --input tutorial/data.json
Model loaded: 98 SID entries across 1 file(s)
Validation passed: tutorial/data.json (544 bytes JSON → 97 bytes CBOR, roundtrip OK)
```

Validation encodes the JSON to CORECONF CBOR, decodes it back, and checks that
the roundtrip preserves the data.  It catches missing or misspelt identifiers.

## 2. Convert JSON ↔ CBOR

```bash
# JSON → CORECONF CBOR (SID-keyed, suitable for CoAP transport)
$ coreconf-cli convert \
    --sid tutorial/coreconf-m2m@2026-03-29.sid \
    --input tutorial/data.json \
    --output data.cbor
Converted tutorial/data.json → data.cbor (544 bytes JSON → 97 bytes CBOR)

# CBOR → JSON (human-readable)
$ coreconf-cli convert --reverse \
    --sid tutorial/coreconf-m2m@2026-03-29.sid \
    --input data.cbor \
    --output roundtrip.json
Converted data.cbor → roundtrip.json (97 bytes CBOR → 667 bytes JSON)
```

## 3. Interactive shell — in-memory datastore

Start with no backing file:

```bash
$ coreconf-cli shell --sid tutorial/coreconf-m2m@2026-03-29.sid
CORECONF interactive shell
Commands: get <path>, set <path> <json-value>, delete <path>, dump, diff, save, reload, quit
```

### Set values inside a keyed list

The transducer list is keyed by `[type, id]`.  The `type` leaf is an identityref
— you write the identity name in the predicate and the library converts it to a
numeric SID internally.

```
coreconf> set /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit "W/m2"
staged
coreconf> set /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision 2
staged
```

### Get a value by predicate path

```
coreconf> get /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit
"W/m2"
```

### Delete a leaf

```
coreconf> delete /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision
staged
coreconf> get /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision
(not found)
```

### Dump the whole datastore

```
coreconf> dump
{
  "coreconf-m2m:transducers": {
    "transducer": [
      {
        "id": 0,
        "type": 100008,
        "unit": "W/m2"
      }
    ]
  }
}
```

In dumps, identityref values appear as their numeric SID (100008 = solar-radiation)
and enumeration values as their integer encoding.

### Errors don't kill the session

```
coreconf> get /coreconf-m2m:nonexistent
error: model error: SID not found for identifier: /coreconf-m2m:nonexistent
coreconf> get /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit
"W/m2"
```

### Multi-entry lists

```
coreconf> set /coreconf-m2m:transducers/transducer[type='coreconf-m2m:air-temperature'][id='0']/unit "degC"
staged
coreconf> dump
{
  "coreconf-m2m:transducers": {
    "transducer": [
      { "id": 0, "type": 100008, "unit": "W/m2" },
      { "id": 0, "type": 100001, "unit": "degC" }
    ]
  }
}
```

## 4. Interactive shell — file-backed datastore

A file-backed session loads an existing JSON file, stages changes in memory,
and saves them when you're ready.  Diffs show what changed since the last save.

```bash
$ echo '{}' > config.json
$ coreconf-cli shell \
    --sid tutorial/coreconf-m2m@2026-03-29.sid \
    --file config.json
```

```
coreconf> set /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit "W/m2"
staged
coreconf> set /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision 2
staged
coreconf> diff
A /coreconf-m2m:transducers {"transducer":[{"id":0,"precision":2,"type":100008,"unit":"W/m2"}]}
coreconf> save
saved config.json
coreconf> reload
reloaded config.json
```

`diff --json` emits machine-readable JSON patches for scripting.

## 5. Live remote session

The `live` command connects to a CORECONF CoAP server, fetches a snapshot,
stages edits locally, and pushes changes via CoAP iPATCH.

Start a reference server in one terminal:

```bash
$ coreconf-cli shell \
    --sid tutorial/coreconf-m2m@2026-03-29.sid \
    --file tutorial/server_config.json \
    --input tutorial/data.json
```

Connect with the live client in another terminal:

```bash
$ coreconf-cli live \
    --sid tutorial/coreconf-m2m@2026-03-29.sid \
    --server [::1]:5683
Connected to coap://[::1]:5683/c
Commands: get <path>, set <path> <json-value>, delete <path>, push, reload, quit
```

```
coreconf-live> get /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit
"W/m2"
coreconf-live> set /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit "kW/m2"
staged
coreconf-live> diff
M /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit "kW/m2"
coreconf-live> push
pushed 1 change(s)
coreconf-live> reload
reloaded from server
```

## 6. Instance-ID FETCH (programmatic API)

A CORECONF FETCH request sends a list of *instance identifiers* — each
identifier pinpoints a specific data node in the YANG tree.  There are
two forms:

| Form | CBOR | Meaning |
|---|---|---|
| Bare SID | `60001` | "the node at SID 60001" (whole subtree or root-level leaf) |
| Instance ID with keys | `[60081, 100008, "0"]` | "the leaf at SID 60081 inside the list entry with key values [100008, '0']" |

The instance-ID form is how you FETCH a leaf inside a keyed list entry.
You build it by calling `resolve_xpath` to get `(target_sid, key_values)`,
then encode `[sid, key1, key2, ...]` as CBOR.

```rust
use coreconf_model::CoreconfModel;
use coreconf_runtime::{Datastore, RequestHandler};
use coreconf_runtime::coap_types::{ContentFormat, Method, Request};

let model = CoreconfModel::new("tutorial/coreconf-m2m@2026-03-29.sid")?;
let mut ds = Datastore::new_in_memory(model.composite_model().clone());

// Create a transducer entry
ds.set_path(
    "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit",
    serde_json::json!("W/m2"),
)?;

let mut handler = RequestHandler::new(ds);

// Resolve a predicate path to (target SID, key values)
let (sid, keys) = handler.datastore().resolve_xpath(
    "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit",
)?;
// sid = 100096 (the 'unit' leaf), keys = [100008, "0"] (identityref SID + string id)

// Build instance ID as CBOR array [sid, key1, key2]
let mut arr = vec![serde_json::Value::Number(sid.into())];
for k in &keys {
    arr.push(k.clone());
}
let mut payload = Vec::new();
ciborium::into_writer(&serde_json::Value::Array(arr), &mut payload).unwrap();

// Send FETCH with the instance ID
let req = Request::new(Method::Fetch)
    .with_payload(payload, ContentFormat::YangIdentifiersCbor);
let resp = handler.handle(&req);
// resp.payload contains the CBOR-encoded value at the target leaf
```

### Decode FETCH responses into a datastore

```rust
// CORECONF map format (GET response or FETCH with bare SIDs)
let ds = Datastore::from_cbor(model, &response_payload)?;

// yang-instances+cbor-seq format (FETCH with Accept: 142)
let ds = Datastore::from_cbor_instance_seq(model, &response_payload)?;
```

## 7. CoAP Observe (streaming notifications)

CORECONF defines two CoAP interfaces:

| Path | Role | Allowed methods |
|---|---|---|
| `/c` | Management — configuration and telemetry | GET, FETCH, iPATCH, POST |
| `/s` | Streaming — time-series and events | FETCH + Observe |

The streaming interface (`/s`) uses RFC 7641 CoAP Observe for push
notifications.  A client registers interest with `Observe=0` on a FETCH
request.  The handler tracks registered observers, marks resources dirty
when data changes (via iPATCH), and provides pending notification sequences
that the CoAP transport layer sends as asynchronous responses.

```rust
use coreconf_runtime::coap_types::{ContentFormat, Interface, Method, Request};
use std::collections::HashSet;

let ds = bootstrapped_datastore();
let mut handler = RequestHandler::new(ds);

// -- Server side: register an observer on the streaming interface --

let token = b"\x01\x02\x03".to_vec();
let mut resources = HashSet::new();
resources.insert("100080".to_string()); // SID of 'precision'

handler.register_observer(token.clone(), resources);

// -- Data changes (e.g. via iPATCH) mark resources dirty --

handler.mark_changed("100080");

// -- Poll for pending notifications --

let notifications = handler.pending_notifications(&token);
for (resource, sequence) in notifications {
    // Build a CoAP notification packet with observe option = sequence
    // Send to the observer's endpoint
}

// -- The transport layer wires this into the streaming handler --
// A FETCH+Observe on /s auto-registers the observer:

let req = Request::new(Method::Fetch)
    .with_interface(Interface::Streaming)
    .with_observe(0)             // register
    .with_payload(payload, ContentFormat::YangIdentifiersCbor);
let resp = handler.handle(&req);
// resp.observe contains the initial sequence number

// Deregister by sending Observe=1
let req = Request::new(Method::Fetch)
    .with_interface(Interface::Streaming)
    .with_observe(1);            // deregister
```
