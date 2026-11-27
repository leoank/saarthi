//! Authorization with Cedar (ADR-0081, phase PHS-20261126-f3e51h item 7): every control call
//! is a request of the schema's actions, allowed or refused by the policy set, against one
//! entity set the hub keeps live as nodes, sessions and accounts change.
//!
//! Until ADR-0082's principals land, the hub knows one person, the owner, an admin: every
//! client in the client-token file is a device of theirs, and every node and Claude account
//! is theirs. The default policies (`default.cedar`) then allow what a client token always
//! could.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use anyhow::{Result, anyhow};
use cedar_policy::{
    Authorizer, Context, Decision, Entities, Entity, EntityUid, PolicySet, Request, Schema, ValidationMode,
    Validator,
};
use saarthi_proto::event::v1::Session;
use serde_json::{Value, json};
use tonic::Status;

/// The schema every request and policy is checked against.
pub const SCHEMA: &str = include_str!("saarthi.cedarschema");
/// The policies the hub starts with.
pub const DEFAULT_POLICIES: &str = include_str!("default.cedar");
/// The one person the hub knows until people are onboarded (ADR-0082).
pub const OWNER: &str = "owner";

/// What a request is about.
#[derive(Debug, Clone, Copy)]
pub enum Res<'a> {
    Hub,
    Node(&'a str),
    Session { node: &'a str, id: &'a str },
    Account(&'a str),
    Shell { node: &'a str, id: &'a str },
}

impl Res<'_> {
    fn uid(&self) -> EntityUid {
        match *self {
            Res::Hub => uid("Hub", "hub"),
            Res::Node(n) => uid("Node", n),
            Res::Session { node, id } => uid("Session", &format!("{node}/{id}")),
            Res::Account(a) => uid("ClaudeAccount", a),
            Res::Shell { node, id } => uid("Shell", &format!("{node}/{id}")),
        }
    }

    /// The resource's kind, as a refusal names it.
    fn kind(&self) -> &'static str {
        match self {
            Res::Hub => "hub",
            Res::Node(_) => "node",
            Res::Session { .. } => "session",
            Res::Account(_) => "Claude account",
            Res::Shell { .. } => "shell",
        }
    }
}

fn uid(kind: &str, id: &str) -> EntityUid {
    EntityUid::from_type_name_and_id(
        format!("Saarthi::{kind}").parse().expect("a schema type"),
        id.parse().expect("an id"),
    )
}

/// An entity reference as Cedar's JSON writes it.
fn r(kind: &str, id: &str) -> Value {
    json!({"__entity": {"type": format!("Saarthi::{kind}"), "id": id}})
}

/// A spawn's or resume's context: the launch profile it runs under on `node` (empty for
/// the host), the Claude account (none for the node's own login) and the model.
pub fn spawn_context(node: &str, profile: &str, account: &str, model: &str) -> Value {
    let mut c = json!({"profile": r("Profile", &format!("{node}/{profile}")), "model": model});
    if !account.is_empty() {
        c["account"] = r("ClaudeAccount", account);
    }
    c
}

/// A resume's or a default account's context: the account, none for the node's own login.
pub fn account_context(account: &str) -> Value {
    if account.is_empty() { json!({}) } else { json!({"account": r("ClaudeAccount", account)}) }
}

pub struct Authz {
    schema: Schema,
    policies: RwLock<Arc<PolicySet>>,
    entities: RwLock<Entities>,
    /// Each entity's JSON as last set, so an unchanged one is not upserted again.
    known: RwLock<HashMap<String, Value>>,
    authorizer: Authorizer,
}

impl Authz {
    /// The engine with `policies`, validated against the schema, and the owner with the
    /// given device clients and Claude accounts.
    pub fn new<'a>(
        policies: &str,
        clients: impl IntoIterator<Item = &'a str>,
        accounts: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self> {
        let schema = Schema::from_str(SCHEMA).map_err(|e| anyhow!("the authorization schema: {e:?}"))?;
        // The schema's actions and their groups (`read`, `operate`) are entities too: without
        // them a policy on a group matches nothing.
        let actions = Entities::from_entities(std::iter::empty(), Some(&schema)).map_err(|e| anyhow!("the schema's actions: {e}"))?;
        let this = Self {
            policies: RwLock::new(Arc::new(validated(&schema, policies)?)),
            schema,
            entities: RwLock::new(actions),
            known: RwLock::new(HashMap::new()),
            authorizer: Authorizer::new(),
        };
        this.upsert(vec![
            ent("Role", "admin", json!({}), &[]),
            ent("Role", "member", json!({}), &[]),
            ent("Role", "monitor", json!({}), &[]),
            ent("Person", OWNER, json!({"name": OWNER}), &[("Role", "admin")]),
            ent("Hub", "hub", json!({}), &[]),
        ])?;
        this.upsert(
            clients
                .into_iter()
                .map(|c| ent("Client", c, json!({"kind": "device"}), &[("Person", OWNER)]))
                .collect(),
        )?;
        for a in accounts {
            this.set_account(a)?;
        }
        Ok(this)
    }

    /// Whether `client` may do `action` on `res` in `context`; a refusal says which action
    /// on what kind of thing, and is logged.
    pub fn check(&self, client: &str, action: &str, res: Res, context: Value) -> Result<(), Status> {
        match self.decide(client, action, res, context) {
            Ok(true) => Ok(()),
            Ok(false) => {
                tracing::info!(%client, %action, resource = %res.uid(), "refused");
                Err(Status::permission_denied(format!("not allowed: {action} on this {}", res.kind())))
            }
            // A request the schema does not admit is refused the same way (ADR-0081).
            Err(e) => {
                tracing::warn!(%client, %action, resource = %res.uid(), error = %format!("{e:#}"), "refused: the request does not fit the schema");
                Err(Status::permission_denied(format!("not allowed: {action} on this {}", res.kind())))
            }
        }
    }

    /// [`Self::check`] as a yes or no, for filtering a list: a refusal there is not logged.
    pub fn allows(&self, client: &str, action: &str, res: Res, context: Value) -> bool {
        self.decide(client, action, res, context).unwrap_or(false)
    }

    fn decide(&self, client: &str, action: &str, res: Res, context: Value) -> Result<bool> {
        let action = EntityUid::from_type_name_and_id("Saarthi::Action".parse()?, action.parse()?);
        let context = Context::from_json_value(context, Some((&self.schema, &action)))
            .map_err(|e| anyhow!("context: {e}"))?;
        let request = Request::new(uid("Client", client), action, res.uid(), context, Some(&self.schema))
            .map_err(|e| anyhow!("request: {e}"))?;
        let policies = self.policies.read().unwrap().clone();
        let entities = self.entities.read().unwrap();
        let answer = self.authorizer.is_authorized(&request, &policies, &entities);
        Ok(answer.decision() == Decision::Allow)
    }

    /// A node of the owner's, with its launch profiles (name, sandboxed); the host's profile,
    /// the empty name, always.
    pub fn set_node(&self, node: &str, profiles: &[(String, bool)]) -> Result<()> {
        let mut ents = vec![ent("Node", node, json!({"owner": r("Person", OWNER), "shared_with": []}), &[])];
        for (name, sandboxed) in std::iter::once((String::new(), false)).chain(profiles.iter().cloned()) {
            ents.push(ent("Profile", &format!("{node}/{name}"), json!({"node": r("Node", node), "sandboxed": sandboxed}), &[]));
        }
        self.upsert(ents)
    }

    /// A session as stored: its node, its owner (the owner's, for now), who launched it and
    /// its labels as tags.
    pub fn set_session(&self, node: &str, s: &Session) -> Result<()> {
        self.upsert(vec![session_entity(node, s)])
    }

    /// Every node and session the hub has recorded, at start, in one update.
    pub async fn fill(&self, db: &crate::db::Db) -> Result<()> {
        let mut ents = Vec::new();
        for n in db.nodes().await? {
            ents.push(ent("Node", &n.id, json!({"owner": r("Person", OWNER), "shared_with": []}), &[]));
            ents.push(ent("Profile", &format!("{}/", n.id), json!({"node": r("Node", &n.id), "sandboxed": false}), &[]));
        }
        let sessions = db.sessions("", true, &Default::default()).await?;
        ents.extend(sessions.iter().map(|s| session_entity(&s.node_id, s)));
        tracing::info!(entities = ents.len(), "authorization entities loaded");
        self.upsert(ents)
    }

    pub fn set_account(&self, name: &str) -> Result<()> {
        self.upsert(vec![ent("ClaudeAccount", name, json!({"owner": r("Person", OWNER), "shared_with": []}), &[])])
    }

    /// A shell on a node, before a request about it.
    pub fn set_shell(&self, node: &str, id: &str) -> Result<()> {
        self.upsert(vec![ent("Shell", &format!("{node}/{id}"), json!({"node": r("Node", node)}), &[])])
    }

    /// Whether the session is in the entity set yet.
    pub fn knows_session(&self, node: &str, id: &str) -> bool {
        self.known.read().unwrap().contains_key(&format!("Saarthi::Session|{node}/{id}"))
    }

    /// Puts entities into the live set, those not already there as they are.
    fn upsert(&self, ents: Vec<Value>) -> Result<()> {
        let mut known = self.known.write().unwrap();
        let changed: Vec<Value> = ents
            .into_iter()
            .filter(|e| {
                known.get(&key(e)) != Some(e)
            })
            .collect();
        if changed.is_empty() {
            return Ok(());
        }
        let parsed = changed
            .iter()
            .map(|e| Entity::from_json_value(e.clone(), Some(&self.schema)).map_err(|err| anyhow!("entity {}: {err}", e["uid"])))
            .collect::<Result<Vec<_>>>()?;
        let mut entities = self.entities.write().unwrap();
        *entities = entities
            .clone()
            .upsert_entities(parsed, Some(&self.schema))
            .map_err(|e| anyhow!("entities: {e}"))?;
        for e in changed {
            known.insert(key(&e), e);
        }
        Ok(())
    }
}

/// `text` as a policy set, refused unless it validates against `schema`.
fn validated(schema: &Schema, text: &str) -> Result<PolicySet> {
    let policies = PolicySet::from_str(text).map_err(|e| anyhow!("the policies do not parse: {e}"))?;
    let result = Validator::new(schema.clone()).validate(&policies, ValidationMode::Strict);
    if !result.validation_passed() {
        let errors: Vec<String> = result.validation_errors().map(|e| e.to_string()).collect();
        return Err(anyhow!("the policies do not fit the schema: {}", errors.join("; ")));
    }
    Ok(policies)
}

fn session_entity(node: &str, s: &Session) -> Value {
    let mut e = ent(
        "Session",
        &format!("{node}/{}", s.id),
        json!({"node": r("Node", node), "owner": r("Person", OWNER), "launched_by": s.launched_by}),
        &[],
    );
    e["tags"] = json!(s.labels);
    e
}

/// An entity's key in [`Authz::known`]: its type and id.
fn key(e: &Value) -> String {
    format!("{}|{}", e["uid"]["type"].as_str().unwrap_or_default(), e["uid"]["id"].as_str().unwrap_or_default())
}

fn ent(kind: &str, id: &str, attrs: Value, parents: &[(&str, &str)]) -> Value {
    json!({
        "uid": {"type": format!("Saarthi::{kind}"), "id": id},
        "attrs": attrs,
        "parents": parents.iter().map(|(k, i)| json!({"type": format!("Saarthi::{k}"), "id": i})).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authz() -> Authz {
        let a = Authz::new(DEFAULT_POLICIES, ["phone", "ctl"], ["work"]).unwrap();
        a.set_node("n1", &[("sandbox".into(), true)]).unwrap();
        a.set_session("n1", &Session { id: "s1".into(), launched_by: "client:phone".into(), ..Default::default() }).unwrap();
        a.set_shell("n1", "sh1").unwrap();
        a
    }

    #[test]
    fn the_defaults_and_schema_validate() {
        let schema = Schema::from_str(SCHEMA).unwrap();
        validated(&schema, DEFAULT_POLICIES).unwrap();
    }

    #[test]
    fn the_owners_devices_may_do_what_a_client_token_could() {
        let a = authz();
        let session = Res::Session { node: "n1", id: "s1" };
        for action in ["ViewSession", "StreamSession", "History", "SessionAbout", "Prompt", "Interrupt", "Unqueue", "Resolve", "SetModel", "Kill", "DeleteSession"] {
            a.check("phone", action, session, json!({})).unwrap();
        }
        a.check("ctl", "Resume", session, account_context("work")).unwrap();
        a.check("ctl", "Resume", session, account_context("")).unwrap();
        for action in ["ViewNode", "GetHealth", "SetKeepAwake", "ServiceRequest", "Terminal", "ListShells"] {
            a.check("phone", action, Res::Node("n1"), json!({})).unwrap();
        }
        a.check("phone", "Spawn", Res::Node("n1"), spawn_context("n1", "", "work", "")).unwrap();
        a.check("phone", "Spawn", Res::Node("n1"), spawn_context("n1", "sandbox", "", "opus")).unwrap();
        a.check("phone", "SetDefaultClaudeToken", Res::Node("n1"), account_context("work")).unwrap();
        a.check("phone", "SetDefaultClaudeToken", Res::Node("n1"), account_context("")).unwrap();
        a.check("phone", "KillShell", Res::Shell { node: "n1", id: "sh1" }, json!({})).unwrap();
        for action in ["ViewAccount", "SetClaudeTokenExpiry", "SetClaudeTokenModel"] {
            a.check("phone", action, Res::Account("work"), json!({})).unwrap();
        }
        for action in ["RegisterPush", "Notifications"] {
            a.check("phone", action, Res::Hub, json!({})).unwrap();
        }
    }

    #[test]
    fn a_principal_the_hub_does_not_know_is_refused() {
        let a = authz();
        let e = a.check("stranger", "ViewSession", Res::Session { node: "n1", id: "s1" }, json!({})).unwrap_err();
        assert_eq!(e.code(), tonic::Code::PermissionDenied);
        assert_eq!(e.message(), "not allowed: ViewSession on this session");
        // An action the schema does not have is refused too, not an error.
        assert!(!a.allows("phone", "Nonsense", Res::Hub, json!({})));
    }

    #[test]
    fn a_policy_that_forbids_is_obeyed_over_the_admins_permit() {
        let keep = r#"
            @id("keep")
            forbid (principal, action in [Saarthi::Action::"Kill", Saarthi::Action::"DeleteSession"], resource is Saarthi::Session)
            when { resource.hasTag("keep") && resource.getTag("keep") == "true" };
        "#;
        let a = Authz::new(&format!("{DEFAULT_POLICIES}\n{keep}"), ["phone"], []).unwrap();
        a.set_node("n1", &[]).unwrap();
        let mut s = Session { id: "s1".into(), launched_by: "client:phone".into(), ..Default::default() };
        a.set_session("n1", &s).unwrap();
        let res = Res::Session { node: "n1", id: "s1" };
        assert!(a.allows("phone", "Kill", res, json!({})));
        s.labels.insert("keep".into(), "true".into());
        a.set_session("n1", &s).unwrap();
        assert!(!a.allows("phone", "Kill", res, json!({})));
        assert!(a.allows("phone", "Prompt", res, json!({})));
    }

    #[test]
    fn a_forbid_on_an_action_group_hides_a_session() {
        let hidden = r#"
            @id("hidden")
            forbid (principal, action in Saarthi::Action::"read", resource is Saarthi::Session)
            when { resource.hasTag("hidden") && resource.getTag("hidden") == "true" };
        "#;
        let a = Authz::new(&format!("{DEFAULT_POLICIES}\n{hidden}"), ["phone"], []).unwrap();
        a.set_node("n1", &[]).unwrap();
        let mut s = Session { id: "s1".into(), launched_by: "client:phone".into(), ..Default::default() };
        s.labels.insert("hidden".into(), "true".into());
        a.set_session("n1", &s).unwrap();
        let res = Res::Session { node: "n1", id: "s1" };
        assert!(!a.allows("phone", "ViewSession", res, json!({})));
        assert!(!a.allows("phone", "StreamSession", res, json!({})));
        assert!(a.allows("phone", "Kill", res, json!({})));
    }

    #[test]
    fn a_member_is_held_to_their_own_and_a_monitor_only_reads() {
        // Until ADR-0082's principals land the hub makes neither; built here by hand.
        let a = authz();
        a.upsert(vec![
            ent("Person", "ana", json!({"name": "ana"}), &[("Role", "member")]),
            ent("Client", "ana-phone", json!({"kind": "device"}), &[("Person", "ana")]),
            ent("Person", "mo", json!({"name": "mo"}), &[("Role", "monitor")]),
            ent("Client", "mo-tv", json!({"kind": "device"}), &[("Person", "mo")]),
            ent("Node", "n2", json!({"owner": r("Person", "ana"), "shared_with": []}), &[]),
            ent("Profile", "n2/", json!({"node": r("Node", "n2"), "sandboxed": false}), &[]),
            ent("Session", "n2/s2", json!({"node": r("Node", "n2"), "owner": r("Person", "ana"), "launched_by": "client:ana-phone"}), &[]),
        ])
        .unwrap();
        let owners = Res::Session { node: "n1", id: "s1" };
        let hers = Res::Session { node: "n2", id: "s2" };
        assert!(!a.allows("ana-phone", "ViewSession", owners, json!({})));
        assert!(a.allows("ana-phone", "ViewSession", hers, json!({})));
        assert!(a.allows("ana-phone", "Prompt", hers, json!({})));
        assert!(!a.allows("ana-phone", "Spawn", Res::Node("n1"), spawn_context("n1", "", "", "")));
        // Her own node's login is hers to spawn under; the owner's Claude account is not.
        assert!(a.allows("ana-phone", "Spawn", Res::Node("n2"), spawn_context("n2", "", "", "")));
        assert!(!a.allows("ana-phone", "Spawn", Res::Node("n2"), spawn_context("n2", "", "work", "")));
        assert!(a.allows("mo-tv", "ViewSession", owners, json!({})));
        assert!(a.allows("mo-tv", "StreamSession", hers, json!({})));
        assert!(!a.allows("mo-tv", "Kill", owners, json!({})));
        assert!(!a.allows("mo-tv", "Terminal", Res::Node("n1"), json!({})));
    }

    #[test]
    fn policies_that_do_not_fit_the_schema_are_refused() {
        let bad = r#"permit (principal, action, resource) when { resource.no_such_attribute == 1 };"#;
        let e = Authz::new(bad, ["phone"], []).err().expect("refused");
        assert!(format!("{e:#}").contains("do not fit the schema"), "{e:#}");
    }
}
