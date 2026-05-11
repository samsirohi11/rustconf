# CORECONF CLI Tutorial

A walkthrough of every operation using the weather-station YANG model
(`coreconf-m2m@2026-03-29.sid`).  This model defines transducers with
identityref-typed keys, enumerations, nested containers, and streaming
data structures — a realistic constrained-device management schema.

All examples use the SID file at:
```
crates/coreconf-runtime/tests/fixtures/coreconf-m2m@2026-03-29.sid
```

## 1. Validate a SID file and data

```bash
$ coreconf-cli validate --sid coreconf-m2m@2026-03-29.sid --input data.json
Model loaded: 98 SID entries across 1 file(s)
Validation passed: data.json (123 bytes JSON → 36 bytes CBOR, roundtrip OK)
```

## 2. Convert JSON ↔ CBOR

```bash
# JSON → CORECONF CBOR
$ coreconf-cli convert --sid coreconf-m2m@2026-03-29.sid --input data.json --output data.cbor
Converted data.json → data.cbor (123 bytes JSON → 36 bytes CBOR)

# CBOR → JSON
$ coreconf-cli convert --reverse --sid coreconf-m2m@2026-03-29.sid --input data.cbor --output roundtrip.json
Converted data.cbor → roundtrip.json (36 bytes CBOR → 193 bytes JSON)
```

## 3. Interactive shell — in-memory datastore

Start with no backing file:

```bash
$ coreconf-cli shell --sid coreconf-m2m@2026-03-29.sid
CORECONF interactive shell
Commands: get <path>, set <path> <json-value>, delete <path>, dump, diff, save, reload, quit
```

### Set values inside a keyed list

The transducer list is keyed by `[type, id]`.  The `type` is an identityref;
you use the identity name in the predicate, and the library converts it to a
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

Identityref keys appear as their numeric SID in dumps (100008 = solar-radiation).

### Errors don't kill the session

```
coreconf> get /coreconf-m2m:nonexistent
error: model error: SID not found for identifier: /coreconf-m2m:nonexistent
coreconf> get /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit
"W/m2"
```

## 4. Interactive shell — file-backed datastore

A file-backed session persists changes and tracks diffs between the saved state
and staged edits.

```bash
$ echo '{}' > config.json
$ coreconf-cli shell --sid coreconf-m2m@2026-03-29.sid --file config.json
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

## 5. Live remote session

The live command connects to a CORECONF CoAP server.  Start a reference server
first (in a separate terminal):

```bash
$ coreconf-cli shell --sid coreconf-m2m@2026-03-29.sid --file server_config.json
```

Then connect with the live client:

```bash
$ coreconf-cli live --sid coreconf-m2m@2026-03-29.sid --server [::1]:5683
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

## 6. Instance-ID FETCH (programmatic)

The library supports instance-ID-based FETCH with list-key navigation:

```rust
use coreconf_model::CoreconfModel;
use coreconf_runtime::{Datastore, RequestHandler};
use coreconf_runtime::coap_types::{ContentFormat, Method, Request};

let model = CoreconfModel::new("coreconf-m2m@2026-03-29.sid")?;
let mut ds = Datastore::new_in_memory(model.composite_model().clone());

// Create a transducer entry
ds.set_path(
    "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit",
    serde_json::json!("W/m2"),
)?;

let mut handler = RequestHandler::new(ds);

// Build instance ID: [target_sid, key1, key2]
let (sid, keys) = handler.datastore().resolve_xpath(
    "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit",
)?;

// Encode as CBOR array [sid, key1, key2] and send FETCH
let payload = /* cbor encode [sid, key1, key2] */;
let req = Request::new(Method::Fetch)
    .with_payload(payload, ContentFormat::YangIdentifiersCbor);
let resp = handler.handle(&req);
// resp.payload contains the unit value
```

## 7. Decode FETCH responses

```rust
// CORECONF map format (GET response)
let ds = Datastore::from_cbor(model, &get_response_payload)?;

// yang-instances+cbor-seq format (FETCH with Accept: 142)
let ds = Datastore::from_cbor_instance_seq(model, &fetch_response_payload)?;
```

## 8. Observe lifecycle

The streaming interface (`/s`) supports CoAP Observe for push notifications:

```rust
use coreconf_runtime::coap_types::{Interface, Method, Request};

// Register for observe on /s
let req = Request::new(Method::Fetch)
    .with_interface(Interface::Streaming)
    .with_observe(0);

// Later, after data changes:
let notifications = handler.pending_notifications(&token);
for (resource, sequence) in notifications {
    // Send notification packet with observe sequence number
}
```
