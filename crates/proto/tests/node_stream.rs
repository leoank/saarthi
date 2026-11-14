//! The generated node service carries a Link stream end to end over loopback: register,
//! an event, its ack and a command, with the token in the metadata.

use std::pin::Pin;

use saarthi_proto::event::v1::{Event, EventSource, Resolution, resolution};
use saarthi_proto::node::v1::node_service_client::NodeServiceClient;
use saarthi_proto::node::v1::node_service_server::{NodeService, NodeServiceServer};
use saarthi_proto::node::v1::{
    Ack, Command, HubMessage, NodeMessage, Register, Registered, command, hub_message, node_message,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

struct Hub;

#[tonic::async_trait]
impl NodeService for Hub {
    type LinkStream = Pin<Box<dyn Stream<Item = Result<HubMessage, Status>> + Send>>;

    async fn claude_token(
        &self,
        _: Request<saarthi_proto::node::v1::ClaudeTokenRequest>,
    ) -> Result<Response<saarthi_proto::node::v1::ClaudeTokenReply>, Status> {
        Err(Status::unimplemented("not part of this test"))
    }

    type ClaudeApiStream = Pin<Box<dyn Stream<Item = Result<saarthi_proto::node::v1::ClaudeApiResponse, Status>> + Send>>;

    async fn claude_api(
        &self,
        _: Request<Streaming<saarthi_proto::node::v1::ClaudeApiRequest>>,
    ) -> Result<Response<Self::ClaudeApiStream>, Status> {
        Err(Status::unimplemented("not part of this test"))
    }

    async fn link(
        &self,
        request: Request<Streaming<NodeMessage>>,
    ) -> Result<Response<Self::LinkStream>, Status> {
        let token = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        if token != Some("Bearer test-token") {
            return Err(Status::unauthenticated("bad token"));
        }
        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(Ok(msg)) = inbound.next().await {
                let reply = match msg.body {
                    Some(node_message::Body::Register(_)) => {
                        hub_message::Body::Registered(Registered {
                            node_id: "n1".into(),
                            last_seq: 0,
                        })
                    }
                    Some(node_message::Body::Event(e)) => {
                        tx.send(Ok(HubMessage {
                            body: Some(hub_message::Body::Ack(Ack { seq: e.seq })),
                        }))
                        .await
                        .ok();
                        hub_message::Body::Command(Command {
                            id: "c1".into(),
                            session_id: e.session_id,
                            action: Some(command::Action::Resolve(Resolution {
                                pending_id: "p1".into(),
                                decision: Some(resolution::Decision::Allow(Default::default())),
                            })),
                        })
                    }
                    _ => continue,
                };
                if tx.send(Ok(HubMessage { body: Some(reply) })).await.is_err() {
                    break;
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

#[tokio::test]
async fn link_round_trip() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(NodeServiceServer::new(Hub))
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );

    let mut client = NodeServiceClient::connect(format!("http://{addr}"))
        .await
        .unwrap();
    let outbound = tokio_stream::iter(vec![
        NodeMessage {
            body: Some(node_message::Body::Register(Register {
                name: "test-node".into(),
                ..Default::default()
            })),
        },
        NodeMessage {
            body: Some(node_message::Body::Event(Event {
                session_id: "s1".into(),
                seq: 1,
                source: EventSource::Hook.into(),
                kind: "PermissionRequest".into(),
                payload_json: "{}".into(),
                ..Default::default()
            })),
        },
    ]);
    let mut request = Request::new(outbound);
    request
        .metadata_mut()
        .insert("authorization", "Bearer test-token".parse().unwrap());
    let mut inbound = client.link(request).await.unwrap().into_inner();

    let mut bodies = Vec::new();
    for _ in 0..3 {
        bodies.push(inbound.next().await.unwrap().unwrap().body.unwrap());
    }
    assert!(matches!(&bodies[0], hub_message::Body::Registered(r) if r.node_id == "n1"));
    assert!(matches!(&bodies[1], hub_message::Body::Ack(a) if a.seq == 1));
    assert!(matches!(
        &bodies[2],
        hub_message::Body::Command(Command { session_id, action: Some(command::Action::Resolve(_)), .. })
            if session_id == "s1"
    ));
}

#[tokio::test]
async fn link_without_token_is_refused() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(NodeServiceServer::new(Hub))
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );

    let mut client = NodeServiceClient::connect(format!("http://{addr}"))
        .await
        .unwrap();
    let status = client.link(tokio_stream::empty()).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}
