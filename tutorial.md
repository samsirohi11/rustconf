# CORECONF Tutorial — Weather Station (coreconf-m2m)

Walkthrough of every CORECONF operation using the coreconf-m2m weather-station
YANG model.

## Setup

You need the SID file and sample data from the `tutorial/` directory:

```
tutorial/
  coreconf-m2m@2026-03-29.sid    # YANG SID file (98 items)
  data.json                       # 12 transducers with statistics
```

Build everything once:

```bash
cargo build --workspace
```

---

## 1. CLI: Convert & Validate

### Convert JSON → CBOR

```bash
cargo run -p coreconf-cli -- convert \
  --sid tutorial/coreconf-m2m@2026-03-29.sid \
  --input tutorial/data.json \
  --output tutorial/data.cbor
# => Converted tutorial/data.json → tutorial/data.cbor (3154 bytes JSON → ~200 bytes CBOR)
```

### Convert CBOR → JSON (roundtrip)

```bash
cargo run -p coreconf-cli -- convert --reverse \
  --sid tutorial/coreconf-m2m@2026-03-29.sid \
  --input tutorial/data.cbor \
  --output tutorial/roundtrip.json
```

### Validate a SID file

```bash
cargo run -p coreconf-cli -- validate \
  --sid tutorial/coreconf-m2m@2026-03-29.sid
```

---

## 2. Library: Datastore Operations

All operations below are demonstrated in the interactive `shell` and `live` CLI
commands. The same API is available programmatically via `coreconf_runtime::Datastore`.

### Start a file-backed shell

```bash
cargo run -p coreconf-cli -- shell \
  --sid tutorial/coreconf-m2m@2026-03-29.sid \
  --file tutorial/data.json
```

### List all transducers (predicates)

```
coreconf> get /coreconf-m2m:transducers/transducer
```

Returns all 12 transducer entries.

### Read a single entry by identityref + id predicates

```
coreconf> get /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']
```

Both identity names (`type='coreconf-m2m:solar-radiation'`) and raw SID
numbers (`type='100008'`) are accepted.

### Read a leaf value

```
coreconf> get /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit
# => "W/m2"

coreconf> get /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/value
# => 6640

coreconf> get /coreconf-m2m:transducers/transducer[type='coreconf-m2m:air-temperature'][id='0']/quantity/statistics/sample-count
# => 8947
```

### Set a leaf value

```
coreconf> set /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision 5
# => staged
```

The change is staged locally. Use `diff` to review and `save` to persist.

### Set an entire list entry

```
coreconf> set /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0'] {"type":"coreconf-m2m:solar-radiation","id":0,"unit":"kW/m2","precision":2,"quantity":{"value":7000,"timestamp":1700000000}}
# => staged
```

### Create a new list entry

```
coreconf> set /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='1']/precision 3
# => auto-creates the list entry with key predicates
```

### Delete a leaf

```
coreconf> delete /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='1']/precision
# => staged
```

### Delete an entire list entry

```
coreconf> delete /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='1']
# => staged
```

### Review changes

```
coreconf> diff
# M /coreconf-m2m:transducers/transducer [before] -> [after]
```

### Save to disk

```
coreconf> save
# Saved tutorial/data.json
```

### Reload from disk (discard staged changes)

```
coreconf> reload
```

---

## 3. CoAP: Server + Live Client

### Start the CoAP server

Terminal 1:

```bash
cargo run -p coreconf-cli -- serve \
  --sid tutorial/coreconf-m2m@2026-03-29.sid \
  --data tutorial/data.json \
  --port 5683
# CORECONF server listening on coap://0.0.0.0:5683
```

### Connect with the live client

Terminal 2:

```bash
cargo run -p coreconf-cli -- live \
  --sid tutorial/coreconf-m2m@2026-03-29.sid \
  --server 127.0.0.1:5683
```

All shell operations (get/set/delete/diff) work identically against the remote
server. Changes are staged locally until `push`.

### Full live-cycle

```
coreconf-live> get /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit
# => "W/m2"

coreconf-live> set /coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision 7
# => staged

coreconf-live> push
# => pushed 1 change(s)  (CoAP iPATCH to server)

coreconf-live> reload
# => reloaded from server (CoAP GET fresh snapshot)
```

### Enumerate all entries via predicates (programmatic)

The `predicates()` API (available in Rust) returns filter strings for every
list entry so you can iterate:

```rust
let preds = datastore.predicates("/coreconf-m2m:transducers/transducer")?;
// => ["[type='coreconf-m2m:solar-radiation'][id='0']", "[type='coreconf-m2m:precipitation'][id='0']", ...]

for pred in &preds {
    let path = format!("/coreconf-m2m:transducers/transducer{pred}/quantity/statistics/sample-count");
    let count = datastore.get_path(&path)?;
    datastore.set_path(&path, count.unwrap().as_i64().unwrap() + 1)?;
}
```

---

## 4. Library: XPath Resolution Roundtrip

```rust
let (sid, keys) = datastore.resolve_xpath(
    "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/value"
)?;
// => sid=100092, keys=[100008, 0]

let roundtrip = datastore.create_xpath(sid, &keys)?;
// => "/transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/value"
```

---

## 5. Library: CBOR Encode/Decode

```rust
let model = CoreconfModel::new("tutorial/coreconf-m2m@2026-03-29.sid")?;

// JSON → CBOR
let cbor = model.to_coreconf(&json_string)?;

// CBOR → JSON
let json = model.to_json(&cbor_bytes)?;

// Create a Datastore from CBOR
let ds = Datastore::from_cbor(model, &cbor)?;

// Or from a FETCH response (yang-instances+cbor-seq)
let ds = Datastore::from_cbor_instance_seq(model, &fetch_payload)?;

// Export datastore as CBOR
let cbor = ds.get_all_cbor()?;
```

---

## 6. CoAP Protocol Test

Test the full CORECONF protocol against the running server with an ad-hoc
Python client (requires `aiocoap` + `cbor2`):

```python
import asyncio, aiocoap, cbor2

async def test():
    ctx = await aiocoap.Context.create_client_context()

    # GET /c — full datastore
    req = aiocoap.Message(code=aiocoap.GET)
    req.opt.uri_path = ('c',)
    req.unresolved_remote = '127.0.0.1'
    req.opt.uri_port = 5683
    resp = await ctx.request(req).response
    print(f"GET → {resp.code}, {len(resp.payload)} bytes CBOR")

    # FETCH /c — single transducer by SID
    req = aiocoap.Message(code=aiocoap.FETCH)
    req.opt.uri_path = ('c',)
    req.opt.content_format = 141   # yang-identifiers+cbor
    req.payload = cbor2.dumps(100029)  # SID of /transducers/transducer
    req.unresolved_remote = '127.0.0.1'
    req.opt.uri_port = 5683
    resp = await ctx.request(req).response
    print(f"FETCH → {resp.code}, {len(resp.payload)} bytes CBOR")

    await ctx.shutdown()

asyncio.run(test())
```

---

## Summary

| Operation                  | CLI         | Library API                      | CoAP Protocol |
| -------------------------- | ----------- | -------------------------------- | ------------- |
| SID file parsing           | `validate`  | `CoreconfModel::new`             | —             |
| JSON ↔ CBOR conversion     | `convert`   | `encode_json_to_cbor` / `decode` | —             |
| Datastore from CBOR        | —           | `Datastore::from_cbor`           | GET response  |
| Predicate-path read        | `shell` get | `get_path`                       | GET /c/path   |
| Predicate-path write       | `shell` set | `set_path`                       | iPATCH        |
| Predicate-path delete      | `shell` del | `delete_path`                    | iPATCH (null) |
| List keys / predicates     | `shell` get | `predicates`                     | FETCH (SID)   |
| Create list entry          | `shell` set | `set_path` (auto-creates)        | iPATCH        |
| XPath ↔ SID+keys roundtrip | —           | `resolve_xpath` / `create_xpath` | —             |
| Remote live session (CoAP) | `live`      | `LiveSession` + `CoapLiteClient` | GET + iPATCH  |
| CoAP server                | `serve`     | `CoapLiteServer::bind`           | all methods   |
