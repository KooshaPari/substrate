//! A tiny, network-free stand-in for the `forge` CLI.
//!
//! Recognised invocations:
//! * `-p <prompt> --agent forge -C <dir>` -> prints a fixed conversation-id.
//! * `conversation dump <id>` -> prints a canned JSON dump.

const CONV_ID: &str = "11111111-1111-1111-1111-111111111111";

const DUMP_JSON: &str = r#"{
  "conversation_id": "11111111-1111-1111-1111-111111111111",
  "exit_code": 0,
  "messages": [
    { "role": "user", "content": "echo hi" },
    { "role": "assistant", "content": "Sure. DONE: printed hi to stdout." }
  ]
}"#;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.first().map(String::as_str) == Some("conversation")
        && args.get(1).map(String::as_str) == Some("dump")
    {
        println!("{DUMP_JSON}");
        return;
    }

    if args.iter().any(|a| a == "-p") {
        println!("conversation-id: {CONV_ID}");
        return;
    }

    eprintln!("fake-forge: unrecognised invocation: {args:?}");
    std::process::exit(2);
}
