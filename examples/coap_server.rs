//! CORECONF CoAP Server CLI
//!
//! Usage:
//!   cargo run --example coap_server -- --sid model.sid [--data initial.json] [--port 5683]
//!

use clap::{Parser, Subcommand};
use coap_lite::{
    CoapRequest, ContentFormat as CoapContentFormat, MessageClass, Packet, RequestType,
    ResponseType,
};
use rust_coreconf::coap_types::{ContentFormat, Method, Request};
use rust_coreconf::{CoreconfModel, Datastore, RequestHandler};
use std::net::UdpSocket;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Parser, Debug)]
#[command(name = "coreconf-server")]
#[command(about = "CORECONF CoAP Server - Serve YANG data via CoAP")]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the SID file (.sid JSON)
    #[arg(short, long, global = true)]
    sid: Option<String>,

    /// Path to initial data file (JSON, optional)
    #[arg(short, long)]
    data: Option<String>,

    /// UDP port to listen on
    #[arg(short, long, default_value = "5683")]
    port: u16,

    /// Resource path for the datastore
    #[arg(long, default_value = "c")]
    path: String,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List all SIDs in the model
    List,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    match args.command {
        Some(Commands::List) => {
            let sid_file = args.sid.expect("--sid is required");
            list_sids(&sid_file);
        }
        None => {
            let sid_file = args.sid.expect("--sid is required to run server");
            run_server(
                &sid_file,
                args.data.as_deref(),
                args.port,
                &args.path,
                args.verbose,
            )?;
        }
    }

    Ok(())
}

fn list_sids(sid_path: &str) {
    let model = CoreconfModel::new(sid_path).expect("Failed to load SID file");

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  SID Mappings for: {:<42} ║", model.sid_file.module_name);
    println!("╠══════════════════════════════════════════════════════════════╣");

    let mut items: Vec<_> = model.sid_file.sids.iter().collect();
    items.sort_by_key(|(_, sid)| *sid);

    for (path, sid) in items {
        let type_str = model
            .sid_file
            .get_type(path)
            .map(|t| format!("{:?}", t))
            .unwrap_or_default();
        println!("║  {:>6}  {:<40} {:>8} ║", sid, path, type_str);
    }
    println!("╚══════════════════════════════════════════════════════════════╝");
}

fn run_server(
    sid_path: &str,
    data_path: Option<&str>,
    port: u16,
    res_path: &str,
    verbose: bool,
) -> std::io::Result<()> {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║              CORECONF CoAP Server                            ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Load SID file
    println!("Loading SID file: {}", sid_path);
    let model = CoreconfModel::new(sid_path).expect("Failed to load SID file");
    println!("  Module: {}", model.sid_file.module_name);
    println!("  Items: {} SIDs loaded", model.sid_file.sids.len());

    // Determine output file path
    let output_path = data_path
        .map(|p| {
            let path = std::path::Path::new(p);
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let parent = path.parent().unwrap_or(std::path::Path::new("."));
            parent.join(format!("{}_modified.json", stem))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("datastore_modified.json"));

    // Load initial data
    let datastore = if let Some(data_file) = data_path {
        println!("\nLoading data file: {}", data_file);
        let json = std::fs::read_to_string(data_file).expect("Failed to read data file");
        Datastore::from_json(model.clone(), &json).expect("Failed to parse data JSON")
    } else {
        println!("\nNo initial data file - starting with empty datastore");
        Datastore::new(model.clone())
    };

    let mut handler = RequestHandler::new(datastore);

    // Set up Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("\n\nReceived Ctrl+C, shutting down...");
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl+C handler");

    // Bind socket
    let bind_addr = format!("0.0.0.0:{}", port);
    let socket = UdpSocket::bind(&bind_addr)?;
    socket.set_read_timeout(Some(std::time::Duration::from_millis(500)))?;

    println!("\n────────────────────────────────────────────────────────────────");
    println!("Server listening on: coap://0.0.0.0:{}", port);
    println!("Datastore resource:  /{}", res_path);
    println!("Output on close:     {}", output_path.display());
    println!("────────────────────────────────────────────────────────────────");
    println!("\nQuick test:");
    println!(
        "  coap-client -m get coap://127.0.0.1:{}/{}",
        port, res_path
    );
    println!("  cargo run --example coap_server -- list -s {}", sid_path);
    println!("\nWaiting for requests... (Ctrl+C to save and stop)\n");

    let mut buf = [0u8; 1500];

    while running.load(Ordering::SeqCst) {
        let (len, src) = match socket.recv_from(&mut buf) {
            Ok(r) => r,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::Interrupted =>
            {
                continue; // Check running flag (also handles Ctrl+C interrupt)
            }
            Err(e) => return Err(e),
        };

        match Packet::from_bytes(&buf[..len]) {
            Ok(packet) => {
                // Skip empty ACK packets (follow-up confirmations)
                if matches!(packet.header.code, MessageClass::Empty) {
                    continue;
                }

                let request = CoapRequest::from_packet(packet, src);
                let path = request.get_path();

                // Skip requests not matching our path
                if path != res_path {
                    let response = create_not_found(&request.message);
                    let bytes = response.to_bytes().unwrap_or_default();
                    socket.send_to(&bytes, src)?;
                    if verbose {
                        println!(
                            "[{}] {} /{} → 4.04 Not Found",
                            src,
                            format_method(&request.message.header.code),
                            path
                        );
                    }
                    continue;
                }

                if verbose {
                    println!(
                        "[{}] {} /{} ({} bytes)",
                        src,
                        format_method(&request.message.header.code),
                        path,
                        request.message.payload.len()
                    );
                    if !request.message.payload.is_empty() {
                        println!("  ← CBOR: {}", hex::encode(&request.message.payload));
                    }
                }

                let response_packet = handle_coap_request(&mut handler, &request, verbose, &model);
                let response_bytes = response_packet.to_bytes().unwrap_or_default();
                socket.send_to(&response_bytes, src)?;

                if verbose {
                    if !response_packet.payload.is_empty() {
                        println!("  → CBOR: {}", hex::encode(&response_packet.payload));
                    }
                    println!(
                        "  → {} ({} bytes)\n",
                        format_response(&response_packet.header.code),
                        response_packet.payload.len()
                    );
                } else {
                    print!(".");
                    use std::io::Write;
                    std::io::stdout().flush().ok();
                }
            }
            Err(_) => {}
        }
    }

    // Save datastore to JSON on close
    println!("\n────────────────────────────────────────────────────────────────");
    println!("Saving datastore to {}...", output_path.display());
    let data = handler.datastore().get_all();
    let json = serde_json::to_string_pretty(data).expect("Failed to serialize datastore");
    std::fs::write(&output_path, &json).expect("Failed to write output file");
    println!("✓ Saved {} bytes", json.len());
    println!("────────────────────────────────────────────────────────────────");

    Ok(())
}

fn handle_coap_request(
    handler: &mut RequestHandler,
    coap_request: &CoapRequest<std::net::SocketAddr>,
    verbose: bool,
    model: &CoreconfModel,
) -> Packet {
    let packet = &coap_request.message;

    let method = match packet.header.code {
        MessageClass::Request(RequestType::Get) => Some(Method::Get),
        MessageClass::Request(RequestType::Post) => Some(Method::Post),
        MessageClass::Request(RequestType::Fetch) => Some(Method::Fetch),
        MessageClass::Request(RequestType::Patch) | MessageClass::Request(RequestType::IPatch) => {
            Some(Method::IPatch)
        }
        _ => None,
    };

    let method = match method {
        Some(m) => m,
        None => return create_method_not_allowed(packet),
    };

    let mut request = Request::new(method);
    request.payload = packet.payload.clone();

    if let Some(cf) = packet.get_content_format() {
        if let Some(format) = content_format_from_coap(cf) {
            request.content_format = Some(format);
        }
    }

    let coreconf_response = handler.handle(&request);

    if verbose && !coreconf_response.payload.is_empty() {
        if let Ok(json) = model.to_json_pretty(&coreconf_response.payload) {
            println!("  Response data:");
            for line in json.lines().take(15) {
                println!("    {}", line);
            }
            if json.lines().count() > 15 {
                println!("    ... (truncated)");
            }
        }
    }

    let mut response = Packet::new();
    response.header.message_id = packet.header.message_id;
    response.set_token(packet.get_token().to_vec());

    let (class, detail) = coreconf_response.code.to_code_pair();
    response.header.code = match (class, detail) {
        (2, 1) => MessageClass::Response(ResponseType::Created),
        (2, 4) => MessageClass::Response(ResponseType::Changed),
        (2, 5) => MessageClass::Response(ResponseType::Content),
        (4, 0) => MessageClass::Response(ResponseType::BadRequest),
        (4, 4) => MessageClass::Response(ResponseType::NotFound),
        (4, 5) => MessageClass::Response(ResponseType::MethodNotAllowed),
        (4, 9) => MessageClass::Response(ResponseType::Conflict),
        _ => MessageClass::Response(ResponseType::InternalServerError),
    };

    if !coreconf_response.payload.is_empty() {
        response.payload = coreconf_response.payload;
        if let Some(format) = coreconf_response.content_format {
            response.set_content_format(content_format_to_coap(format));
        }
    }

    response
}

fn create_not_found(request: &Packet) -> Packet {
    let mut response = Packet::new();
    response.header.message_id = request.header.message_id;
    response.header.code = MessageClass::Response(ResponseType::NotFound);
    response.set_token(request.get_token().to_vec());
    response
}

fn create_method_not_allowed(request: &Packet) -> Packet {
    let mut response = Packet::new();
    response.header.message_id = request.header.message_id;
    response.header.code = MessageClass::Response(ResponseType::MethodNotAllowed);
    response.set_token(request.get_token().to_vec());
    response
}

fn format_method(code: &MessageClass) -> &'static str {
    match code {
        MessageClass::Request(RequestType::Get) => "GET",
        MessageClass::Request(RequestType::Post) => "POST",
        MessageClass::Request(RequestType::Put) => "PUT",
        MessageClass::Request(RequestType::Delete) => "DELETE",
        MessageClass::Request(RequestType::Fetch) => "FETCH",
        MessageClass::Request(RequestType::Patch) => "PATCH",
        MessageClass::Request(RequestType::IPatch) => "iPATCH",
        MessageClass::Empty => "EMPTY",
        _ => "???",
    }
}

fn format_response(code: &MessageClass) -> String {
    match code {
        MessageClass::Response(r) => format!("{:?}", r),
        _ => "???".to_string(),
    }
}

fn content_format_from_coap(cf: CoapContentFormat) -> Option<ContentFormat> {
    match cf {
        CoapContentFormat::ApplicationCBOR => Some(ContentFormat::YangDataCbor),
        _ => None,
    }
}

fn content_format_to_coap(format: ContentFormat) -> CoapContentFormat {
    match format {
        ContentFormat::YangDataCbor => CoapContentFormat::ApplicationCBOR,
        ContentFormat::YangInstancesCborSeq => CoapContentFormat::ApplicationCBOR,
        ContentFormat::YangIdentifiersCbor => CoapContentFormat::ApplicationCBOR,
    }
}
