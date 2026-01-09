//! CORECONF Explained - Demonstrates the protocol with decoded output
//!
//! Run with: cargo run --example explained
//!
//! This example shows what's happening at each step of CORECONF operations.

use rust_coreconf::coap_types::{ContentFormat, Method, Request};
use rust_coreconf::{CoreconfModel, Datastore, RequestBuilder, RequestHandler};
use serde_json::json;

const SAMPLE_SID: &str = r#"{
    "assignment-range": [{"entry-point": 60000, "size": 10}],
    "module-name": "example-1",
    "module-revision": "unknown",
    "item": [
        {"namespace": "module", "identifier": "example-1", "sid": 60000},
        {"namespace": "data", "identifier": "/example-1:greeting", "sid": 60001},
        {"namespace": "data", "identifier": "/example-1:greeting/author", "sid": 60002, "type": "string"},
        {"namespace": "data", "identifier": "/example-1:greeting/message", "sid": 60003, "type": "string"}
    ],
    "key-mapping": {}
}"#;

const INITIAL_DATA: &str = r#"{
    "example-1:greeting": {
        "author": "Obi-Wan",
        "message": "Hello there!"
    }
}"#;

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                    CORECONF Protocol Explained                       ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    // ========== SETUP ==========
    println!("┌─ SETUP ──────────────────────────────────────────────────────────────┐");
    println!("│ CORECONF uses YANG data models with SID (YANG Schema Item Identifier)│");
    println!("│ numbers instead of long string names to save bandwidth.              │");
    println!("└──────────────────────────────────────────────────────────────────────┘\n");

    println!("SID Mappings from our .sid file:");
    println!("  60000 = example-1 (module)");
    println!("  60001 = /example-1:greeting (container)");
    println!("  60002 = /example-1:greeting/author (leaf, string)");
    println!("  60003 = /example-1:greeting/message (leaf, string)");
    println!();

    let model = CoreconfModel::from_str(SAMPLE_SID).unwrap();
    let datastore = Datastore::from_json(model.clone(), INITIAL_DATA).unwrap();
    let mut handler = RequestHandler::new(datastore);
    let builder = RequestBuilder::new(model.clone());

    println!("Initial Data (JSON):");
    println!("  {}", INITIAL_DATA.trim());
    println!();

    // ========== GET ==========
    println!("┌─ 1. GET Request ──────────────────────────────────────────────────────┐");
    println!("│ Purpose: Retrieve the ENTIRE datastore                                │");
    println!("│ CoAP:    GET /c                                                       │");
    println!("└───────────────────────────────────────────────────────────────────────┘\n");

    let request = Request::new(Method::Get);
    let response = handler.handle(&request);

    println!("Response Code: {:?} (2.05 Content)", response.code);
    println!(
        "Response CBOR ({} bytes): {}",
        response.payload.len(),
        hex::encode(&response.payload)
    );

    // Decode and display
    let decoded = model.to_json_pretty(&response.payload).unwrap();
    println!("\nDecoded Response (JSON):");
    for line in decoded.lines() {
        println!("  {}", line);
    }
    println!();

    // ========== FETCH ==========
    println!("┌─ 2. FETCH Request ────────────────────────────────────────────────────┐");
    println!("│ Purpose: Retrieve SPECIFIC nodes by SID                               │");
    println!("│ CoAP:    FETCH /c with payload containing SID(s)                      │");
    println!("│ We'll fetch SID 60002 (author field)                                  │");
    println!("└───────────────────────────────────────────────────────────────────────┘\n");

    let fetch_payload = builder.build_fetch_sids(&[60002]).unwrap();
    println!(
        "Request Payload (CBOR): {} = SID 60002",
        hex::encode(&fetch_payload)
    );

    let request =
        Request::new(Method::Fetch).with_payload(fetch_payload, ContentFormat::YangIdentifiersCbor);
    let response = handler.handle(&request);

    println!("\nResponse Code: {:?}", response.code);
    println!("Response CBOR: {}", hex::encode(&response.payload));

    // Decode the instances response
    let instances = rust_coreconf::instance_id::decode_instances(&response.payload).unwrap();
    println!("\nDecoded Response:");
    for inst in &instances {
        if let (Some(sid), Some(value)) = (inst.path.absolute_sid(), &inst.value) {
            let identifier = model.sid_file.get_identifier(sid).unwrap_or("unknown");
            println!("  SID {} ({}) = {}", sid, identifier, value);
        }
    }
    println!();

    // ========== iPATCH ==========
    println!("┌─ 3. iPATCH Request ───────────────────────────────────────────────────┐");
    println!("│ Purpose: MODIFY data nodes (create, update, or delete)                │");
    println!("│ CoAP:    iPATCH /c with payload {{SID: new_value}}                      │");
    println!("│ We'll change author from 'Obi-Wan' to 'General Kenobi'                │");
    println!("└───────────────────────────────────────────────────────────────────────┘\n");

    let patch_payload = builder
        .build_ipatch_sids(&[(60002, Some(json!("General Kenobi")))])
        .unwrap();

    println!("Request Payload (CBOR): {}", hex::encode(&patch_payload));
    println!("  Meaning: {{60002: \"General Kenobi\"}}");

    let request = Request::new(Method::IPatch)
        .with_payload(patch_payload, ContentFormat::YangInstancesCborSeq);
    let response = handler.handle(&request);

    println!(
        "\nResponse Code: {:?} (2.04 Changed = success!)",
        response.code
    );
    println!();

    // ========== VERIFY ==========
    println!("┌─ 4. GET to Verify Change ─────────────────────────────────────────────┐");
    println!("│ Let's GET the full datastore again to see our change                  │");
    println!("└───────────────────────────────────────────────────────────────────────┘\n");

    let request = Request::new(Method::Get);
    let response = handler.handle(&request);

    let decoded = model.to_json_pretty(&response.payload).unwrap();
    println!("Decoded Response (JSON):");
    for line in decoded.lines() {
        println!("  {}", line);
    }
    println!();

    // ========== DELETE ==========
    println!("┌─ 5. iPATCH with null = DELETE ─────────────────────────────────────────┐");
    println!("│ Purpose: Delete a node by setting it to null                           │");
    println!("│ Payload: {{60002: null}}                                                 │");
    println!("└────────────────────────────────────────────────────────────────────────┘\n");

    let delete_payload = builder
        .build_ipatch_sids(&[
            (60002, None), // None = delete
        ])
        .unwrap();

    println!("Request Payload (CBOR): {}", hex::encode(&delete_payload));

    let request = Request::new(Method::IPatch)
        .with_payload(delete_payload, ContentFormat::YangInstancesCborSeq);
    let response = handler.handle(&request);

    println!("Response Code: {:?}\n", response.code);

    // Final state
    let request = Request::new(Method::Get);
    let response = handler.handle(&request);
    let decoded = model.to_json_pretty(&response.payload).unwrap();
    println!("Final State (author deleted):");
    for line in decoded.lines() {
        println!("  {}", line);
    }

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                           Summary                                    ║");
    println!("╠══════════════════════════════════════════════════════════════════════╣");
    println!("║ GET      - Retrieve entire datastore                                 ║");
    println!("║ FETCH    - Retrieve specific nodes by SID                            ║");
    println!("║ iPATCH   - Create/Update nodes: {{SID: value}}                         ║");
    println!("║           Delete nodes: {{SID: null}}                                  ║");
    println!("║ POST     - Invoke RPCs/Actions                                       ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
}
