//! A headless S3 client over the code the apps move files with - azul-storage's S3 drive (parts
//! of 16 MiB four at once, a file's upload resumable after a kill, ranged downloads four at once
//! that resume) behind azcloud-kit's failover (the block endpoint, a node's hint, the node list,
//! retries by the class of the answer) - over the bridge's HTTP client (ureq + rustls, no
//! libazul). scripts/s3_transfer_e2e.py runs it against the local S3 test server.
//!
//! ```text
//! s3_transfer put <file> <key>             a file, resumable (--resume DIR)
//! s3_transfer get <key> <file>             into a file, resumable (a hidden .part beside it)
//! s3_transfer put-if-absent <key> <text>   a conditional write (If-None-Match: *)
//! s3_transfer put-file-if-absent <file> <key>  a streamed conditional write
//!
//! --endpoint URL     the block endpoint (required)
//! --node URL[,IP...] a node to fail over to (repeatable, in order), with the addresses it is
//!                    reached at when its name does not resolve (the block host at all of them)
//! --bucket NAME      (required)
//! --access-key K --secret-key S --region R (default us-east-1)
//! --resume DIR       where the state files of resumable uploads go
//! --part-size BYTES  the part size (default 16 MiB)
//! --parallel N       parts / ranges at once (default 4)
//! ```
//!
//! Prints one JSON line. Exit code 0: done; 3: another writer won (a conflict); 1: anything
//! else.

use std::{path::PathBuf, process::ExitCode, sync::Arc, time::Duration};

use azcloud_kit::failover::{Failover, Node, Retry};
use azul_bridge::transport::UreqTransport;
use azul_storage::{transfer, Credentials, Drive, DriveError, Precondition, S3Config, S3Drive};
use serde_json::json;

/// The command line, as given.
struct Args {
    command: String,
    rest: Vec<String>,
    endpoint: String,
    nodes: Vec<String>,
    bucket: String,
    access_key: String,
    secret_key: String,
    region: String,
    resume: Option<PathBuf>,
    part_size: usize,
    parallel: usize,
}

fn parse() -> Result<Args, String> {
    let mut words = std::env::args().skip(1);
    let command = words.next().ok_or("a command: put, get, put-if-absent")?;
    let mut args = Args {
        command,
        rest: Vec::new(),
        endpoint: String::new(),
        nodes: Vec::new(),
        bucket: String::new(),
        access_key: String::new(),
        secret_key: String::new(),
        region: String::from("us-east-1"),
        resume: None,
        part_size: azul_storage::s3::PART_SIZE,
        parallel: azul_storage::s3::PARALLEL_PARTS,
    };
    while let Some(word) = words.next() {
        let mut value = || words.next().ok_or(format!("{word} needs a value"));
        match word.as_str() {
            "--endpoint" => args.endpoint = value()?,
            "--node" => args.nodes.push(value()?),
            "--bucket" => args.bucket = value()?,
            "--access-key" => args.access_key = value()?,
            "--secret-key" => args.secret_key = value()?,
            "--region" => args.region = value()?,
            "--resume" => args.resume = Some(PathBuf::from(value()?)),
            "--part-size" => {
                args.part_size = value()?.parse().map_err(|_| "--part-size takes bytes")?;
            }
            "--parallel" => {
                args.parallel = value()?.parse().map_err(|_| "--parallel takes a number")?;
            }
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            _ => args.rest.push(word.clone()),
        }
    }
    if args.endpoint.is_empty() || args.bucket.is_empty() {
        return Err(String::from("--endpoint and --bucket are needed"));
    }
    Ok(args)
}

/// The bucket through the failover, as an Azlin drive's is.
fn drive(args: &Args) -> Result<S3Drive, DriveError> {
    let failover = Failover::new(&args.endpoint);
    failover.set_nodes(
        args.nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                // `URL[,ADDRESS...]`: where the node is reached when its name does not resolve.
                let mut parts = node.split(',').map(str::trim);
                let url = parts.next().unwrap_or_default().to_string();
                Node {
                    name: format!("n{}", i + 1),
                    url,
                    addresses: parts.filter(|a| !a.is_empty()).map(String::from).collect(),
                    ready: true,
                    iroh_id: None,
                    iroh_addrs: Vec::new(),
                }
            })
            .collect(),
    );
    failover.set_retry(Retry {
        rounds: 3,
        backoff: Duration::from_millis(200),
        max_pause: Duration::from_secs(5),
    });
    let drive = S3Drive::new(
        S3Config {
            endpoint: args.endpoint.clone(),
            region: args.region.clone(),
            bucket: args.bucket.clone(),
            path_style: true,
        },
        Credentials::new(&args.access_key, &args.secret_key),
        Box::new(UreqTransport::new()),
    )?
    .with_router(Arc::new(failover))
    .with_part_size(args.part_size)
    .with_parallel(args.parallel);
    Ok(match &args.resume {
        Some(dir) => drive.with_resume_dir(dir),
        None => drive,
    })
}

fn run(args: &Args) -> Result<serde_json::Value, DriveError> {
    let drive = drive(args)?;
    let rest = |i: usize| {
        args.rest.get(i).cloned().ok_or_else(|| {
            DriveError::InvalidConfig(format!("{} needs more arguments", args.command))
        })
    };
    match args.command.as_str() {
        "put" => {
            let (file, key) = (PathBuf::from(rest(0)?), rest(1)?);
            let sent = drive.put_file(&key, &file, &|_| {})?;
            Ok(json!({"ok": true, "key": key, "bytes": sent}))
        }
        "get" => {
            let (key, file) = (rest(0)?, PathBuf::from(rest(1)?));
            let got = transfer::download_to_file(&drive, &key, None, &file, args.part_size as u64)?;
            Ok(json!({"ok": true, "key": key, "bytes": got}))
        }
        "put-if-absent" => {
            let (key, text) = (rest(0)?, rest(1)?);
            let etag = drive.put_if(&key, text.as_bytes(), &Precondition::Absent)?;
            Ok(json!({"ok": true, "key": key, "etag": etag}))
        }
        "put-file-if-absent" => {
            let (file, key) = (PathBuf::from(rest(0)?), rest(1)?);
            let mut body = std::fs::File::open(&file)?;
            let etag = drive.put_from_if(&key, &mut body, &Precondition::Absent)?;
            Ok(json!({"ok": true, "key": key, "etag": etag}))
        }
        other => Err(DriveError::InvalidConfig(format!(
            "unknown command {other}"
        ))),
    }
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(args) => args,
        Err(why) => {
            println!("{}", json!({"ok": false, "error": why}));
            return ExitCode::from(1);
        }
    };
    match run(&args) {
        Ok(done) => {
            println!("{done}");
            ExitCode::SUCCESS
        }
        Err(DriveError::Conflict { key }) => {
            println!("{}", json!({"ok": false, "conflict": true, "key": key}));
            ExitCode::from(3)
        }
        Err(e) => {
            println!("{}", json!({"ok": false, "error": e.to_string()}));
            ExitCode::from(1)
        }
    }
}
