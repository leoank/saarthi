# Apps

An app of yours can use the hub as the phone does: start Claude Code sessions for its own
work on your machines, follow them, answer what they ask and collect what they did. It is
a client of the hub's control service, the same gRPC API the app and `saarthi ctl` use,
defined in `crates/proto/proto/saarthi/control/v1/control.proto`.

> **For now an app is a client like any other.** Its token is a line in the hub's
> client-token file, and a client token may do everything the phone can, on every node.
> Tokens of its own, held to a grant of nodes, launch profiles and Claude accounts, come
> with the hub's authorization (ADR-0081, ADR-0082). Until then give an app a token of its
> own, so its sessions say which app started them.

## A token

Add a line for the app to the hub's client-token file and restart the hub:

```text
reports 9c1e…
```

The name, `reports`, is how the hub knows the app: sessions it starts are launched by
`client:reports`.

## From Rust

Until the client library is published (phase item 14), depend on the generated client in
`saarthi-proto` by git revision, the same one as your hub's. Building it needs `protoc`.

```toml
[dependencies]
saarthi-proto = { git = "https://github.com/leoank/saarthi", rev = "<the hub's commit>" }
tonic = "0.14"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
use saarthi_proto::control::v1::{control_service_client::ControlServiceClient, *};

fn req<T>(body: T, token: &str) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut().insert("authorization", format!("Bearer {token}").parse().unwrap());
    r
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = std::fs::read_to_string("/run/secrets/reports-token")?.trim().to_string();
    let mut hub = ControlServiceClient::connect("http://hub:7700").await?;

    // Which node to run on, and what the hub calls this app.
    let nodes = hub.list_nodes(req(ListNodesRequest {}, &token)).await?.into_inner();
    let node = nodes.nodes.iter().find(|n| n.name == "ghost001" && n.connected).ok_or("ghost001 is offline")?;
    println!("this app is {}", nodes.you); // client:reports

    // A session for job 1234, labelled so it can be found again.
    let spawned = hub
        .spawn(req(
            SpawnRequest {
                node_id: node.id.clone(),
                cwd: "/srv/reports/1234".into(),
                create: true,
                prompt: "Summarise yesterday's failures in failures.md".into(),
                labels: [("job".to_string(), "1234".to_string())].into(),
                ..Default::default()
            },
            &token,
        ))
        .await?
        .into_inner();
    if !spawned.ok {
        return Err(spawned.error.into());
    }
    println!("started {}", spawned.session_id);

    // Later: this app's sessions for that job, live or ended.
    let mine = hub
        .list_sessions(req(
            ListSessionsRequest {
                include_ended: true,
                launched_by: nodes.you.clone(),
                labels: [("job".to_string(), "1234".to_string())].into(),
                ..Default::default()
            },
            &token,
        ))
        .await?
        .into_inner()
        .sessions;
    for s in mine {
        println!("{} {:?} ended={}", s.id, s.status(), s.ended_ms.is_some());
    }
    Ok(())
}
```

## Labels

A spawn may carry up to 16 labels, `key = value`, to match a session to the app's own
work: a job, a ticket, a run. Keys are 1 to 64 letters, digits or `_.-/`; values up to
256 characters. They come back on every `Session` as `labels`, a resume keeps them, and
a session a resume forks takes them too. Labels are the app's; who launched a session is
the hub's alone (`Session.launched_by`), and no request can set it.

## Finding your sessions

`ListSessions`, `History`, `Search` and `Watch` each take the same filter:

- `launched_by`: one launcher exactly (`client:reports`), or every launcher of a kind,
  ending in `:` (`client:` for every client or app, `node:` for sessions started by hand
  at a machine). `ListNodes` returns this client's own as `you`.
- `labels`: only sessions with every one of these.

`Watch` with a filter still sends every node and request update, and session updates only
for the sessions it keeps. `History` lists sessions with a transcript, so one that has not
started a turn yet is in `ListSessions` but not in `History`.

From a shell, `saarthi ctl` does the same:

```console
$ saarthi ctl spawn ghost001 /srv/reports/1234 "go" --create --label job=1234
$ saarthi ctl sessions --all --by client:reports --label job=1234
```

## Running sessions where the app runs

To run sessions on a machine of the app's own, without a saarthi daemon there, or apart
from the daemon's, the app can [embed a node](embedded-node.md).
