use std::env;
use std::fs;

use nerve_protocol::codec::{decode, encode};
use nerve_protocol::message::{
    AiToken, SearchContext, SearchOptions, SearchQuery, decode_message, encode_message,
};
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

const WS_URL: &str = "ws://127.0.0.1:9001/";

fn main() {
    let mut args = env::args().skip(1);

    let (query, token_override) = parse_args(&mut args);

    let token = match token_override {
        Some(t) => t,
        None => {
            let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            let path = format!("{}/.anvesha/token", home);
            fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read token from {path}: {e}"))
                .trim()
                .to_string()
        }
    };

    let mut request = WS_URL
        .into_client_request()
        .expect("failed to build WebSocket request");

    request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        format!("anvesha-v1.{}", token)
            .parse()
            .expect("invalid protocol header value"),
    );

    let (mut ws, _) =
        tungstenite::connect(request).expect("failed to connect — is anvesha-daemon running?");

    println!("connected to {WS_URL}");

    send_search_query(&mut ws, &query);
    println!("sent SearchQuery: {query:?}");
    println!("---");

    receive_stream(&mut ws);
}

fn parse_args(args: &mut impl Iterator<Item = String>) -> (String, Option<String>) {
    let mut query = "hello".to_string();
    let mut token: Option<String> = None;

    while let Some(arg) = args.next() {
        if arg == "--token" {
            token = args.next();
        } else if !arg.starts_with('-') {
            query = arg;
        }
    }

    (query, token)
}

fn send_search_query(ws: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>, query: &str) {
    let sq = SearchQuery {
        v: 1,
        query: query.to_string(),
        context: SearchContext {
            url: "http://localhost/".to_string(),
            title: "ws_client manual test".to_string(),
            selection: None,
            extract: None,
        },
        opts: SearchOptions::default(),
    };

    let payload = encode_message(&sq).expect("SearchQuery JSON encode failed");
    let frame = encode(
        MessageType::SearchQuery,
        FrameFlags::FINAL,
        RequestId(1),
        &payload,
    )
    .expect("NERVE frame encode failed");

    ws.send(Message::Binary(frame))
        .expect("WebSocket send failed");
}

fn receive_stream(ws: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>) {
    loop {
        let msg = match ws.read() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("read error: {e}");
                break;
            }
        };

        let data = match msg {
            Message::Binary(b) => b,
            Message::Close(_) => {
                println!("server closed connection");
                break;
            }
            Message::Ping(_) | Message::Pong(_) => continue,
            other => {
                eprintln!("unexpected WebSocket message variant: {:?}", other);
                continue;
            }
        };

        let frame = match decode(&data) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("NERVE decode error: {e}");
                continue;
            }
        };

        let msg_type = MessageType::try_from(frame.header.msg_type).ok();
        let flags = FrameFlags::from_bits_truncate(frame.header.flags);
        let request_id = frame.header.request_id;

        println!("frame: msg_type={msg_type:?}  request_id={request_id}  flags={flags:?}");

        if matches!(msg_type, Some(MessageType::AiToken)) {
            match decode_message::<AiToken>(frame.payload) {
                Ok(tok) => print!("{}", tok.t),
                Err(e) => eprintln!("AiToken decode error: {e}"),
            }
        }

        if flags.contains(FrameFlags::FINAL) && matches!(msg_type, Some(MessageType::AiToken)) {
            println!("\n--- stream complete ---");
            break;
        }
    }
}
