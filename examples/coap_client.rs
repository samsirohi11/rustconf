//! Interactive CORECONF CoAP Client
//!
//! A REPL-style CoAP client for interacting with a CORECONF server.
//!
//! Usage:
//!   cargo run --example coap_client -- --sid model.sid [--server coap://127.0.0.1:5683/c]
//!
//! Commands:
//!   get                    - Get full datastore
//!   fetch <sid1> [sid2...] - Fetch specific SIDs
//!   set <sid>=<value>      - Set a value
//!   delete <sid>           - Delete a value
//!   list                   - Show all SIDs
//!   help                   - Show commands
//!   quit                   - Exit

use clap::Parser;
use coap_lite::{
    ContentFormat as CoapContentFormat, MessageClass, MessageType, Packet, RequestType,
};
use rust_coreconf::coap_types::ContentFormat;
use rust_coreconf::{CoreconfModel, RequestBuilder};
use std::io::{self, Write};
use std::net::UdpSocket;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "coap-client")]
#[command(about = "Interactive CORECONF CoAP Client")]
struct Args {
    /// Path to the SID file (.sid JSON)
    #[arg(short, long)]
    sid: String,

    /// Server address
    #[arg(long, default_value = "127.0.0.1:5683")]
    server: String,

    /// Resource path
    #[arg(long, default_value = "c")]
    path: String,
}

struct Client {
    model: CoreconfModel,
    builder: RequestBuilder,
    socket: UdpSocket,
    path: String,
    message_id: u16,
}

impl Client {
    fn new(model: CoreconfModel, server: &str, path: &str) -> io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(Duration::from_secs(5)))?;
        socket.connect(server)?;

        Ok(Self {
            builder: RequestBuilder::new(model.clone()),
            model,
            socket,
            path: path.to_string(),
            message_id: 1,
        })
    }

    fn send_request(
        &mut self,
        request_type: RequestType,
        payload: Vec<u8>,
        content_format: Option<ContentFormat>,
    ) -> io::Result<Option<Vec<u8>>> {
        let mut packet = Packet::new();
        packet.header.message_id = self.message_id;
        self.message_id = self.message_id.wrapping_add(1);
        packet.header.code = MessageClass::Request(request_type);
        packet.header.set_type(MessageType::Confirmable);
        packet.set_token(vec![0x01]);
        packet.add_option(
            coap_lite::CoapOption::UriPath,
            self.path.as_bytes().to_vec(),
        );

        if !payload.is_empty() {
            packet.payload = payload;
            if let Some(format) = content_format {
                let cf = match format {
                    ContentFormat::YangDataCbor => CoapContentFormat::ApplicationCBOR,
                    ContentFormat::YangIdentifiersCbor => CoapContentFormat::ApplicationCBOR,
                    ContentFormat::YangInstancesCborSeq => CoapContentFormat::ApplicationCBOR,
                };
                packet.set_content_format(cf);
            }
        }

        let bytes = packet
            .to_bytes()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        self.socket.send(&bytes)?;

        let mut buf = [0u8; 1500];
        match self.socket.recv(&mut buf) {
            Ok(len) => {
                let response = Packet::from_bytes(&buf[..len])
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

                let code_str = format!("{:?}", response.header.code);
                println!("  Response: {}", code_str);

                if response.payload.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(response.payload))
                }
            }
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                println!("  Timeout - no response");
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    fn cmd_get(&mut self) {
        println!("GET /{}", self.path);
        match self.send_request(RequestType::Get, vec![], None) {
            Ok(Some(payload)) => self.decode_and_print(&payload),
            Ok(None) => {}
            Err(e) => println!("  Error: {}", e),
        }
    }

    fn cmd_fetch(&mut self, sids: Vec<i64>) {
        println!("FETCH SIDs: {:?}", sids);
        for sid in &sids {
            match self.model.sid_file.get_identifier(*sid) {
                Some(path) => println!("  {} = {}", sid, path),
                None => println!("  {} = (unknown SID)", sid),
            }
        }

        match self.builder.build_fetch_sids(&sids) {
            Ok(payload) => {
                match self.send_request(
                    RequestType::Fetch,
                    payload,
                    Some(ContentFormat::YangIdentifiersCbor),
                ) {
                    Ok(Some(response)) => self.decode_instances(&response),
                    Ok(None) => println!("  (no data returned)"),
                    Err(e) => println!("  Error: {}", e),
                }
            }
            Err(e) => println!("  Failed to build request: {}", e),
        }
    }

    fn cmd_set(&mut self, changes: Vec<(i64, serde_json::Value)>) {
        println!("iPATCH (SET):");
        for (sid, value) in &changes {
            if let Some(path) = self.model.sid_file.get_identifier(*sid) {
                println!("  {} ({}) = {}", sid, path, value);
            }
        }

        let changes: Vec<_> = changes.into_iter().map(|(sid, v)| (sid, Some(v))).collect();
        match self.builder.build_ipatch_sids(&changes) {
            Ok(payload) => {
                match self.send_request(
                    RequestType::IPatch,
                    payload,
                    Some(ContentFormat::YangInstancesCborSeq),
                ) {
                    Ok(_) => println!("  ✓ Done"),
                    Err(e) => println!("  Error: {}", e),
                }
            }
            Err(e) => println!("  Failed to build request: {}", e),
        }
    }

    fn cmd_delete(&mut self, sids: Vec<i64>) {
        println!("iPATCH (DELETE):");
        for sid in &sids {
            if let Some(path) = self.model.sid_file.get_identifier(*sid) {
                println!("  {} ({})", sid, path);
            }
        }

        let changes: Vec<_> = sids.into_iter().map(|sid| (sid, None)).collect();
        match self.builder.build_ipatch_sids(&changes) {
            Ok(payload) => {
                match self.send_request(
                    RequestType::IPatch,
                    payload,
                    Some(ContentFormat::YangInstancesCborSeq),
                ) {
                    Ok(_) => println!("  ✓ Deleted"),
                    Err(e) => println!("  Error: {}", e),
                }
            }
            Err(e) => println!("  Failed to build request: {}", e),
        }
    }

    fn cmd_list(&self) {
        println!("\nSID Mappings:");
        println!("─────────────────────────────────────────────────────────");
        let mut items: Vec<_> = self.model.sid_file.sids.iter().collect();
        items.sort_by_key(|(_, sid)| *sid);

        for (path, sid) in items {
            let type_str = self
                .model
                .sid_file
                .get_type(path)
                .map(|t| format!("{:?}", t))
                .unwrap_or_default();
            println!("  {:>6}  {:<40} {}", sid, path, type_str);
        }
        println!();
    }

    fn decode_and_print(&self, payload: &[u8]) {
        match self.model.to_json_pretty(payload) {
            Ok(json) => {
                println!("  Data:");
                for line in json.lines() {
                    println!("    {}", line);
                }
            }
            Err(e) => {
                println!("  CBOR ({} bytes): {}", payload.len(), hex::encode(payload));
                println!("  Decode error: {}", e);
            }
        }
    }

    fn decode_instances(&self, payload: &[u8]) {
        match rust_coreconf::instance_id::decode_instances(payload) {
            Ok(instances) => {
                if instances.is_empty() {
                    println!("  (no data for requested SIDs)");
                } else {
                    println!("  Results:");
                    for inst in instances {
                        if let Some(sid) = inst.path.absolute_sid() {
                            let path = self.model.sid_file.get_identifier(sid).unwrap_or("?");
                            let value = inst
                                .value
                                .map(|v| v.to_string())
                                .unwrap_or("null".to_string());
                            println!("    {} ({}) = {}", sid, path, value);
                        }
                    }
                }
            }
            Err(_) => self.decode_and_print(payload),
        }
    }
}

fn print_help() {
    println!("\nCommands:");
    println!("  get                        Get full datastore");
    println!("  fetch <sid1> [sid2...]     Fetch specific SIDs");
    println!("  set <sid>=<value>          Set a value (e.g., set 60002=\"Hello\")");
    println!("  delete <sid>               Delete a SID");
    println!("  list                       Show all SIDs in the model");
    println!("  help                       Show this help");
    println!("  quit                       Exit");
    println!();
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║           CORECONF Interactive Client                     ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    // Load SID file
    println!("Loading: {}", args.sid);
    let model = CoreconfModel::new(&args.sid).expect("Failed to load SID file");
    println!("Module:  {}", model.sid_file.module_name);
    println!("Server:  {}", args.server);
    println!("Path:    /{}\n", args.path);

    let mut client = Client::new(model, &args.server, &args.path)?;

    println!("Type 'help' for commands, 'quit' to exit.\n");

    loop {
        print!("coreconf> ");
        io::stdout().flush()?;

        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 {
            break; // EOF
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        let (cmd, rest) = match input.split_once(char::is_whitespace) {
            Some((c, r)) => (c.to_lowercase(), r.trim()),
            None => (input.to_lowercase(), ""),
        };

        match cmd.as_str() {
            "quit" | "exit" | "q" => {
                println!("Bye!");
                break;
            }
            "help" | "?" => print_help(),
            "get" => client.cmd_get(),
            "list" | "ls" => client.cmd_list(),
            "fetch" | "f" => {
                if rest.is_empty() {
                    println!("Usage: fetch <sid1> [sid2...]");
                } else {
                    let sids: Vec<i64> = rest
                        .split_whitespace()
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    if sids.is_empty() {
                        println!("No valid SIDs provided");
                    } else {
                        client.cmd_fetch(sids);
                    }
                }
            }
            "set" | "s" => {
                if rest.is_empty() {
                    println!("Usage: set <sid>=<value>");
                } else {
                    // Parse set command preserving spaces in quoted values
                    // Format: set <sid>=<value>  (value can contain spaces if quoted)
                    let mut changes = Vec::new();
                    if let Some((sid_str, val_str)) = rest.split_once('=') {
                        if let Ok(sid) = sid_str.trim().parse::<i64>() {
                            // Try to parse as JSON first, otherwise treat as string
                            let value: serde_json::Value = serde_json::from_str(val_str.trim())
                                .unwrap_or_else(|_| {
                                    serde_json::Value::String(val_str.trim().to_string())
                                });
                            changes.push((sid, value));
                        }
                    }
                    if changes.is_empty() {
                        println!("No valid changes. Use: set 60002=\"value\"");
                    } else {
                        client.cmd_set(changes);
                    }
                }
            }
            "delete" | "del" | "d" => {
                if rest.is_empty() {
                    println!("Usage: delete <sid1> [sid2...]");
                } else {
                    let sids: Vec<i64> = rest
                        .split_whitespace()
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    if sids.is_empty() {
                        println!("No valid SIDs provided");
                    } else {
                        client.cmd_delete(sids);
                    }
                }
            }
            _ => println!("Unknown command: {}. Type 'help' for commands.", cmd),
        }
        println!();
    }

    Ok(())
}
