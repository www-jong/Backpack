use backpack_core::{scan, scan_at, ScanRequest};
use std::path::PathBuf;

fn main() {
    let mut request = ScanRequest::default();
    let mut home = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => home = args.next().map(PathBuf::from),
            "--project" => request.project_path = args.next(),
            "--help" => {
                println!("backpack-scan [--home <path>] [--project <path>]\n읽기 전용 탐지. 에이전트·도구를 실행하지 않습니다.");
                return;
            }
            _ => {
                eprintln!("지원하지 않는 인자입니다.");
                std::process::exit(2);
            }
        }
    }
    let result = if let Some(home) = home {
        scan_at(&home, request)
    } else {
        scan(request)
    };
    match result {
        Ok(snapshot) => println!(
            "{}",
            serde_json::to_string_pretty(&snapshot).expect("JSON serialization")
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
