//! CORECONF CoAP server command — serves a YANG datastore over CoAP.
//!
//! ```bash
//! coreconf-cli serve --sid model.sid --data datastore.json
//! coreconf-cli serve --sid model.sid --port 5683 --path c -v
//! ```

use clap::Args;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use coap_lite::{CoapOption, MessageClass, Packet, ResponseType};
use coreconf_model::CoreconfModel;
use coreconf_runtime::transport::coap_lite::CoapLiteServer;
use coreconf_runtime::{Datastore, RequestHandler};

use crate::CliError;

/// Start a CORECONF CoAP server backed by a local datastore.
#[derive(Args)]
pub struct ServeArgs {
    /// Path(s) to .sid JSON files describing the YANG model
    #[arg(long, required = true, num_args = 1..)]
    pub sid: Vec<String>,

    /// Path to initial data file (JSON).  Saved to `<data>.modified.json` on Ctrl+C.
    #[arg(long)]
    pub data: Option<PathBuf>,

    /// UDP port to listen on
    #[arg(long, default_value = "5683")]
    pub port: u16,

    /// CORECONF resource path (URI segment before sub-paths)
    #[arg(long, default_value = "c")]
    pub path: String,

    /// Verbose request/response logging
    #[arg(short = 'v', long, default_value_t = false)]
    pub verbose: bool,
}

pub fn run(args: ServeArgs) -> Result<(), CliError> {
    let model = crate::load_model(&args.sid)?;
    let coreconf_model = CoreconfModel::new(&args.sid[0]).map_err(CliError::Model)?;

    let datastore = if let Some(ref data_path) = args.data {
        let json_str = std::fs::read_to_string(data_path).map_err(CliError::Io)?;
        Datastore::from_json(coreconf_model.clone(), &json_str).map_err(CliError::Model)?
    } else {
        Datastore::new_in_memory(model.clone())
    };

    let bind_addr = format!("0.0.0.0:{}", args.port);
    let handler = RequestHandler::new(datastore);
    let mut server =
        CoapLiteServer::bind(&bind_addr, &args.path, handler).map_err(CliError::Model)?;

    eprintln!("CORECONF server listening on coap://0.0.0.0:{}", args.port);
    eprintln!("  Datastore resource: /{}", args.path);
    if let Some(ref dp) = args.data {
        let out = dp.with_extension("modified.json");
        eprintln!("  Output on close:    {}", out.display());
    }
    eprintln!(
        "  Logging:            {}",
        if args.verbose {
            "verbose"
        } else {
            "errors only"
        }
    );
    eprintln!("  Press Ctrl+C to save and stop.");
    eprintln!();

    let socket = server.socket().try_clone().map_err(CliError::Io)?;
    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok();

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .map_err(|e| CliError::Io(std::io::Error::other(e)))?;

    // ── Main serve loop with verbose logging ───────────────────────────────
    let mut buf = [0u8; 1500];

    while running.load(Ordering::SeqCst) {
        let (len, peer) = match socket.recv_from(&mut buf) {
            Ok(v) => v,
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => {
                let _ = writeln!(io::stderr(), "[error] recv: {e}");
                continue;
            }
        };

        let packet = match Packet::from_bytes(&buf[..len]) {
            Ok(p) => p,
            Err(e) => {
                if args.verbose {
                    let _ = writeln!(io::stderr(), "[error] parse: {e}");
                }
                continue;
            }
        };

        // Log incoming request
        if args.verbose {
            let method = match &packet.header.code {
                MessageClass::Request(req) => format!("{req:?}"),
                _other => format!("{_other:?}"),
            };
            let path = packet
                .get_option(CoapOption::UriPath)
                .map(|opts| {
                    opts.iter()
                        .filter_map(|b| std::str::from_utf8(b).ok())
                        .collect::<Vec<_>>()
                        .join("/")
                })
                .unwrap_or_default();
            let _ = writeln!(io::stderr(), "[req] {peer}  {method} /{path}  ({len}B)",);
        }

        // Handle the request
        let response = server.handle_packet(&packet, peer);

        // Log response
        if args.verbose {
            let code_str = match &response.header.code {
                MessageClass::Response(ResponseType::Content) => "2.05",
                MessageClass::Response(ResponseType::Changed) => "2.04",
                MessageClass::Response(ResponseType::Created) => "2.01",
                MessageClass::Response(ResponseType::BadRequest) => "4.00",
                MessageClass::Response(ResponseType::NotFound) => "4.04",
                MessageClass::Response(ResponseType::MethodNotAllowed) => "4.05",
                MessageClass::Response(ResponseType::Conflict) => "4.09",
                MessageClass::Response(ResponseType::UnsupportedContentFormat) => "4.15",
                MessageClass::Response(ResponseType::InternalServerError) => "5.00",
                _ => "?",
            };
            let _ = writeln!(
                io::stderr(),
                "[res] {peer}  {code_str}  ({}B payload)",
                response.payload.len(),
            );
        }

        // Send response
        let bytes = match response.to_bytes() {
            Ok(b) => b,
            Err(e) => {
                let _ = writeln!(io::stderr(), "[error] encode response: {e}");
                continue;
            }
        };

        if let Err(e) = socket.send_to(&bytes, peer) {
            let _ = writeln!(io::stderr(), "[error] send: {e}");
        }

        // Push any pending observer notifications.
        server.flush_pending_notifications();
    }

    // ── Save modified datastore ────────────────────────────────────────────
    if let Some(ref data_path) = args.data {
        let tree = server.handler().datastore().get_all();
        let json = serde_json::to_string_pretty(&tree).map_err(|e| CliError::Model(e.into()))?;
        let output = data_path.with_extension("modified.json");
        std::fs::write(&output, &json).map_err(CliError::Io)?;
        eprintln!("\nDatastore saved to: {}", output.display());
    }

    eprintln!("Server stopped.");
    Ok(())
}
