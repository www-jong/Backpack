use std::{io::{self,Write},time::Duration};
fn main() {
    let mode=std::env::args().nth(1).unwrap_or_default();
    match mode.as_str() {
        "hang"=>std::thread::sleep(Duration::from_secs(60)),
        "flood"=>{println!("{}","x".repeat(1024*1024+1));},
        "fail"=>{eprintln!("SECRET_FAILURE");std::process::exit(1);},
        "normal"=>{println!("safe: https://SECRET.example - Connected"); eprintln!("SECRET_LOG");},
        _=>std::process::exit(2),
    }
    io::stdout().flush().unwrap();
}
