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
                let spit = spitter.spit(&mut rand::rng());
                for (i, stream) in  streams.iter_mut().enumerate() {
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
    for char in ' '..='~' {
        list.push(char.to_string());
    }
    list.push("\r\n".to_string());
    list
}

#[derive(Debug)]
struct RandStringSpitter {
    spit_list: Vec<String>,
    banned_strings: Vec<String>,
    prev_string: String,
}
impl RandStringSpitter {
    pub fn new_ssh_identifier() -> Self {
        Self {
            banned_strings: vec!["SSH-".to_string()],
            spit_list: ascii_string_list(),
            prev_string: String::new(),
        }
    }
    pub fn spit(&mut self, rng: &mut impl Rng) -> &str {
        let spit_list = Choose::new(&self.spit_list).unwrap();
        loop {
            let mut appended_string = self.prev_string.clone();
            let next = rng.sample(&spit_list);
            appended_string.push_str(next.as_str());

            if ends_with_any(&appended_string, &self.banned_strings) {
                continue;
            }

            let extra_prefix = appended_string
                .len()
                .saturating_sub(longest_len(&self.banned_strings));
            appended_string.drain(..extra_prefix);
            self.prev_string = appended_string;
            break next;
        }
    }
}

fn longest_len<S>(strings: &[S]) -> usize
where
    S: AsRef<str>,
{
    strings.iter().map(|s| s.as_ref().len()).max().unwrap_or(0)
}

fn ends_with_any<S>(s: &str, keywords: &[S]) -> bool
where
    S: AsRef<str>,
{
    for keyword in keywords {
        if s.ends_with(keyword.as_ref()) {
            return true;
        }
    }
    false
}
