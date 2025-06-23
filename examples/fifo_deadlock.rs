use nix::{sys::stat::Mode, unistd::mkfifo};
use sfifo::Sfifo;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let fifo_path = "/tmp/";

    let mut cnt = 1;
    loop {
        let std_in = PathBuf::from(fifo_path).join(format!("stdin-fifo"));
        mkfifo(&std_in, Mode::S_IRWXU).unwrap_or_default();
        let std_in2 = std_in.clone();
        let std_in3 = std_in.clone();

        let r2 = tokio::task::spawn(async move {
            println!("open read fifo start");
            let _r = Sfifo::new(&std_in2)
                .open_receiver()
                .await
                .map_err(|e| println!("read err {:?}", e));
            println!("open read fifo end");
        });

        let r3 = tokio::task::spawn(async move {
            println!("remove fifo start");
            let _ = std::fs::remove_file(&std_in3);
            println!("remove fifo end");
        });
        let r1 = tokio::task::spawn(async move {
            println!("open write fifo start");
            let _s = Sfifo::new(&std_in)
                .open_sender()
                .await
                .map_err(|e| println!("write err {:?}", e));
            println!("open write fifo end");
        });
        let _ = tokio::join!(r1, r2, r3);
        println!("success {}", cnt);
        cnt += 1;
        if cnt > 50 {
            break;
        }
    }
}
