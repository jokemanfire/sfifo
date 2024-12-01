use std::{path::PathBuf, fs::OpenOptions};

use nix::{unistd::mkfifo, sys::stat::Mode};

#[tokio::main]
async fn main()  {

    let fifo_path = "/tmp/";

    let mut cnt = 1;
    loop{
        let std_in = PathBuf::from(fifo_path).join("stdin-fifo");
        mkfifo(&std_in, Mode::S_IRWXU).unwrap_or_default();
        let std_in2 = std_in.clone();
        let std_in3 = std_in.clone();


        let r2 = tokio::task::spawn(async move{
            println!("open read fifo start");
            let r = sfifo::Sfifo::new(&std_in2).set_read(true).set_notify(true).open().await;
            println!("{:?}", r);
            println!("open read fifo end");
        });

        let r3 = tokio::task::spawn(async move {
            println!("remove fifo start");
            sfifo::delete_fifo(&std_in3).await;
            println!("remove fifo end");
        });
        let r1 = tokio::task::spawn(async move{
            println!("open write fifo start");
            let r = sfifo::Sfifo::new(&std_in).set_write(true).set_notify(true).open().await;
            println!("{:?}", r);
            println!("open write fifo end");
        });
        r1.await;
        r2.await;
        r3.await;
        println!("success {}",cnt);
        cnt += 1;
    }
   
}