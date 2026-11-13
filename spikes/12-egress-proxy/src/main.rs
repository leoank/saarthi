//! Runs one shell command inside mxc's seatbelt sandbox with the policy saarthi's node uses
//! for a sandboxed profile (crates/node/src/containment.rs: egress allowed, no inbound, no
//! host loopback; read-write only the state directory and the working directory; read-only
//! /nix/store; an environment only of what is passed), and passes its exit code on.
//!
//!     egress-proxy-spike <state dir> <work dir> <command> [NAME=VALUE ...]

use mxc_sdk::v1::configs::SeatbeltConfig;
use mxc_sdk::v1::policy::{FilesystemPolicy, NetworkAction, NetworkEgressPolicy, NetworkIngressPolicy, NetworkPolicy};
use mxc_sdk::v1::{self, ContainerRequest, Containment};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [state, work, command, env @ ..] = args.as_slice() else {
        eprintln!("usage: egress-proxy-spike <state dir> <work dir> <command> [NAME=VALUE ...]");
        std::process::exit(2);
    };
    let env = env
        .iter()
        .map(|kv| {
            let (k, v) = kv.split_once('=').expect("NAME=VALUE");
            (k.to_string(), v.to_string())
        })
        .collect();
    let mut seatbelt = SeatbeltConfig::default();
    seatbelt.extra_mach_lookups = vec!["com.apple.system.opendirectoryd.libinfo".into()];
    let request = ContainerRequest {
        containment: Containment::Seatbelt(seatbelt),
        filesystem: Some(FilesystemPolicy {
            readwrite_paths: vec![state.clone(), work.clone()],
            readonly_paths: vec!["/nix/store".into()],
            denied_paths: vec!["/private/tmp/cc-socks".into()],
            ..Default::default()
        }),
        network: Some(NetworkPolicy {
            egress: Some(NetworkEgressPolicy { default: Some(NetworkAction::Allow), ..Default::default() }),
            ingress: Some(NetworkIngressPolicy {
                default: Some(NetworkAction::Deny),
                host_loopback: Some(NetworkAction::Deny),
            }),
            ..Default::default()
        }),
        environment: Some(env),
        inherit_default_environment: Some(false),
        working_directory: Some(work.clone()),
        ..ContainerRequest::new(command.clone())
    };
    match v1::run(request, Default::default()) {
        Ok(out) => {
            print!("{}", String::from_utf8_lossy(&out.stdout));
            eprint!("{}", String::from_utf8_lossy(&out.stderr));
            match out.outcome {
                v1::WaitResult::Exited(code) => std::process::exit(code),
                other => {
                    eprintln!("sandbox: {other:?}");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("sandbox: {e}");
            std::process::exit(1);
        }
    }
}
