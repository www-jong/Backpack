use std::io::{self, BufRead, Write};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("normal");
    let cwd = args.get(2).map(String::as_str).unwrap_or("\"/tmp\"");
    let path = args.get(3).map(String::as_str).unwrap_or("\"/tmp/SKILL.md\"");
    let mut initialized = false;
    for line in io::stdin().lock().lines().map_while(Result::ok) {
        if line.contains("\"method\":\"initialized\"") { initialized=true; continue; }
        let Some(start) = line.find("\"id\":") else { continue; };
        let id: String = line[start+5..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if id.is_empty() { continue; }
        if mode == "hang" { std::thread::sleep(std::time::Duration::from_secs(60)); }
        if mode == "flood" { println!("{}", "x".repeat(2*1024*1024+10)); io::stdout().flush().unwrap(); continue; }
        if mode == "unsupported" { println!("{{\"id\":{id},\"error\":{{\"code\":-32601,\"message\":\"SECRET_ERROR\"}}}}"); }
        else if mode == "error" { println!("{{\"id\":{id},\"error\":{{\"code\":-32000,\"message\":\"SECRET_ERROR\"}}}}"); }
        else if line.contains("\"method\":\"initialize\"") { println!("{{\"id\":{id},\"result\":{{\"userAgent\":\"codex/0.159.2\"}}}}"); }
        else if !initialized { panic!("Client did not acknowledge initialization"); }
        else if line.contains("\"method\":\"config/read\"") {
            println!("{{\"method\":\"ignored/notification\",\"params\":{{\"secret\":\"SECRET_NOTIFICATION\"}}}}");
            println!("{{\"id\":{id},\"result\":{{\"config\":{{\"model\":\"test-model\",\"api_key\":\"SECRET_CONFIG\",\"env\":{{\"TOKEN\":\"SECRET_ENV\"}}}}}}}}");
        }
        else if line.contains("\"method\":\"skills/list\"") { println!("{{\"id\":{id},\"result\":{{\"data\":[{{\"cwd\":{cwd},\"errors\":[],\"skills\":[{{\"name\":\"custom\",\"path\":{path},\"scope\":\"user\",\"enabled\":false,\"description\":\"SECRET_DESCRIPTION\",\"dependencies\":{{\"tools\":[{{\"url\":\"https://example.test?token=SECRET_URL\"}}]}}}}]}}]}}}}"); }
        else if line.contains("\"method\":\"mcpServerStatus/list\"") { println!("{{\"id\":{id},\"result\":{{\"data\":[{{\"name\":\"custom\",\"authStatus\":\"unknown\",\"runtimeStatus\":null,\"tools\":{{\"lookup\":{{\"description\":\"SECRET_TOOL\",\"inputSchema\":{{\"secret\":\"SECRET_SCHEMA\"}}}}}},\"toolsError\":null}}],\"nextCursor\":null}}}}"); }
        else { panic!("Unexpected method in read-only inspection"); }
        io::stdout().flush().unwrap();
    }
}
