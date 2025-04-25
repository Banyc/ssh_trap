use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use clap::Parser;
use rand::{Rng, distr::slice::Choose};
use rand_distr::Normal;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

#[derive(Debug, Parser)]
struct Cli {
    pub listen_addrs: Vec<SocketAddr>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let (accept_tx, accept_rx) = mpsc::channel(1024);
    tokio::spawn(async move {
        run_writer(accept_rx).await;
    });
    for listen_addr in cli.listen_addrs {
        let listener = TcpListener::bind(listen_addr).await.unwrap();
        let accept_tx = accept_tx.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                };
                accept_tx.send(stream).await.unwrap();
            }
        });
    }
    tokio::signal::ctrl_c().await.unwrap();
}

async fn run_writer(mut accept_rx: mpsc::Receiver<TcpStream>) {
    let mut spitter = RandStringSpitter::new_ssh_identifier();
    let mut sleep_until = new_sleep_until();
    let mut streams = vec![];
    let mut remove = vec![];
    loop {
        tokio::select! {
            stream = accept_rx.recv() => {
                streams.push(stream.unwrap());
            }
            () = tokio::time::sleep_until(sleep_until.into()) => {
                sleep_until = new_sleep_until();
                for (i, stream) in  streams.iter_mut().enumerate() {
                    let spit = spitter.spit(&mut rand::rng());
                    let res = match tokio::time::timeout(Duration::from_secs(1), stream.write_all(spit.as_bytes())).await {
                        Ok(res) => res,
                        Err(_) => {
                            remove.push(i);
                            continue;
                        }
                    };
                    if res.is_err() {
                        remove.push(i);
                    }
                }
                for &i in remove.iter().rev() {
                    streams.swap_remove(i);
                }
                remove.clear();
            }
        }
    }
}

fn new_sleep_until() -> Instant {
    let mut rng = rand::rng();
    let norm = Normal::new(10., 1.).unwrap();
    let dur: f64 = rng.sample(norm);
    Instant::now() + Duration::from_secs_f64(dur.max(0.))
}

fn ascii_string_list() -> Vec<String> {
    let mut list = vec![];
    for char in ' '..'~' {
        list.push(char.to_string());
    }
    list.push("\r\n".to_string());
    list
}

#[derive(Debug)]
struct RandStringSpitter {
    spit_list: Vec<String>,
    banned: Banned,
    prev_string: String,
}
impl RandStringSpitter {
    pub fn new_ssh_identifier() -> Self {
        Self {
            banned: Banned::new(vec!["SSH-".to_string()]),
            spit_list: ascii_string_list(),
            prev_string: String::new(),
        }
    }
    pub fn spit(&mut self, rng: &mut impl Rng) -> &str {
        let spit_list = Choose::new(&self.spit_list).unwrap();
        loop {
            let mut new_string = self.prev_string.clone();
            let next = rng.sample(&spit_list);
            new_string.push_str(next.as_str());

            if self.banned.is_banned(&new_string) {
                continue;
            }

            let drain = new_string.len().saturating_sub(self.banned.ctx_len());
            new_string.drain(..drain);
            self.prev_string = new_string;
            break next;
        }
    }
}

#[derive(Debug)]
struct Banned {
    list: Vec<String>,
}
impl Banned {
    pub fn new(list: Vec<String>) -> Self {
        Self { list }
    }
    pub fn ctx_len(&self) -> usize {
        self.list.iter().map(|s| s.len()).max().unwrap_or(0)
    }
    pub fn is_banned(&self, s: &str) -> bool {
        let mut is_banned = false;
        for banned in &self.list {
            if banned.ends_with(s) {
                is_banned = true;
                break;
            }
        }
        is_banned
    }
}
