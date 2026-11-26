//! EXP-20261126-f3e5a9: can the hub authorize every control call with Cedar? Throwaway.
//!
//! `cedar-spike check` validates the policies against the schema (strict), decides a table of
//! requests against a small world and checks that a policy set swapped at run time applies on
//! the next call. `cedar-spike bench` times authorizations at hub-like sizes. Both print one
//! `===RESULT===` line.

use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use anyhow::{Context as _, Result, bail};
use cedar_policy::{
    Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request, Schema, ValidationMode,
    Validator,
};
use serde_json::{Value, json};

const SCHEMA: &str = include_str!("../saarthi.cedarschema");
const POLICY_FILES: &[(&str, &str)] = &[
    ("admin", include_str!("../policies/admin.cedar")),
    ("members", include_str!("../policies/members.cedar")),
    ("apps", include_str!("../policies/apps.cedar")),
    ("monitor", include_str!("../policies/monitor.cedar")),
    ("nodes", include_str!("../policies/nodes.cedar")),
    ("labels", include_str!("../policies/labels.cedar")),
];

fn uid(t: &str, id: &str) -> EntityUid {
    EntityUid::from_str(&format!("Saarthi::{t}::\"{id}\"")).unwrap()
}
fn r(t: &str, id: &str) -> Value {
    json!({"__entity": {"type": format!("Saarthi::{t}"), "id": id}})
}
fn ent(t: &str, id: &str, attrs: Value, parents: &[(&str, &str)]) -> Value {
    let parents: Vec<Value> =
        parents.iter().map(|(t, id)| json!({"type": format!("Saarthi::{t}"), "id": id})).collect();
    json!({"uid": {"type": format!("Saarthi::{t}"), "id": id}, "attrs": attrs, "parents": parents})
}
fn session(id: &str, node: &str, owner: &str, by: &str, app: Option<&str>, tags: Value) -> Value {
    let mut attrs = json!({"node": r("Node", node), "owner": r("Person", owner), "launched_by": by});
    if let Some(a) = app {
        attrs["app"] = r("App", a);
    }
    let mut e = ent("Session", id, attrs, &[]);
    e["tags"] = tags;
    e
}

fn schema() -> Result<Schema> {
    let (schema, warnings) = Schema::from_cedarschema_str(SCHEMA).context("schema")?;
    for w in warnings {
        eprintln!("schema warning: {w}");
    }
    Ok(schema)
}

fn policies(skip: &[&str]) -> Result<PolicySet> {
    let text: String = POLICY_FILES
        .iter()
        .filter(|(name, _)| !skip.contains(name))
        .map(|(_, p)| *p)
        .collect::<Vec<_>>()
        .join("\n");
    PolicySet::from_str(&text).map_err(|e| anyhow::anyhow!("policies: {e:?}"))
}

/// The world the cases run in: three people, their devices, two apps, four nodes (one
/// embedded in an app), profiles, accounts, sessions and a shell.
fn world() -> Vec<Value> {
    let grant = |nodes: &[&str], profiles: &[&str], accounts: &[&str], terminal: bool| {
        json!({
            "nodes": nodes.iter().map(|n| r("Node", n)).collect::<Vec<_>>(),
            "profiles": profiles.iter().map(|p| r("Profile", p)).collect::<Vec<_>>(),
            "accounts": accounts.iter().map(|a| r("ClaudeAccount", a)).collect::<Vec<_>>(),
            "terminal": terminal,
        })
    };
    let node = |id: &str, owner: &str, shared: &[&str], app: Option<&str>| {
        let mut a = json!({"owner": r("Person", owner),
            "shared_with": shared.iter().map(|p| r("Person", p)).collect::<Vec<_>>()});
        if let Some(app) = app {
            a["app"] = r("App", app);
        }
        ent("Node", id, a, &[])
    };
    let profile = |id: &str, node: &str, sandboxed: bool| {
        ent("Profile", id, json!({"node": r("Node", node), "sandboxed": sandboxed}), &[])
    };
    let account = |id: &str, owner: &str, shared: &[&str]| {
        ent("ClaudeAccount", id, json!({"owner": r("Person", owner),
            "shared_with": shared.iter().map(|p| r("Person", p)).collect::<Vec<_>>()}), &[])
    };
    vec![
        ent("Role", "admin", json!({}), &[]),
        ent("Role", "member", json!({}), &[]),
        ent("Role", "monitor", json!({}), &[]),
        ent("Person", "ank", json!({"name": "ank"}), &[("Role", "admin")]),
        ent("Person", "bea", json!({"name": "bea"}), &[("Role", "member")]),
        ent("Person", "cy", json!({"name": "cy"}), &[("Role", "member")]),
        ent("Client", "ank-phone", json!({"kind": "device"}), &[("Person", "ank")]),
        ent("Client", "bea-phone", json!({"kind": "device"}), &[("Person", "bea")]),
        ent("App", "reviewer", json!({"owner": r("Person", "ank"),
            "grant": grant(&["rogue", "reviewer-node"], &["rogue/host", "reviewer-node/host"], &["mrclmb"], false)}), &[]),
        ent("App", "builder", json!({"owner": r("Person", "bea"),
            "grant": grant(&["bea-box"], &["bea-box/host"], &["bea-acct"], true)}), &[]),
        ent("App", "dash", json!({"owner": r("Person", "ank"),
            "grant": grant(&[], &[], &[], false)}), &[("Role", "monitor")]),
        node("ghost001", "ank", &["bea"], None),
        node("rogue", "ank", &[], None),
        node("bea-box", "bea", &[], None),
        node("reviewer-node", "ank", &[], Some("reviewer")),
        profile("ghost001/host", "ghost001", false),
        profile("rogue/host", "rogue", false),
        profile("rogue/sandbox", "rogue", true),
        profile("bea-box/host", "bea-box", false),
        profile("reviewer-node/host", "reviewer-node", false),
        account("mrclmb", "ank", &[]),
        account("bodygram", "ank", &["bea"]),
        account("bea-acct", "bea", &[]),
        session("s-ank-rogue", "rogue", "ank", "client:ank-phone", None, json!({})),
        session("s-rev-1", "rogue", "ank", "app:reviewer", Some("reviewer"), json!({"job": "1234", "keep": "true"})),
        session("s-bea-ghost", "ghost001", "bea", "client:bea-phone", None, json!({})),
        session("s-bea-box", "bea-box", "bea", "client:bea-phone", None, json!({})),
        session("s-builder", "bea-box", "bea", "app:builder", Some("builder"), json!({})),
        session("s-bare-rogue", "rogue", "ank", "node:rogue", None, json!({})),
        ent("Shell", "sh-rogue-1", json!({"node": r("Node", "rogue")}), &[]),
        ent("Hub", "hub", json!({}), &[]),
    ]
}

struct Case {
    what: &'static str,
    principal: (&'static str, &'static str),
    action: &'static str,
    resource: (&'static str, &'static str),
    context: Value,
    allow: bool,
}

fn spawn_ctx(profile: &str, account: Option<&str>) -> Value {
    let mut c = json!({"profile": r("Profile", profile), "model": ""});
    if let Some(a) = account {
        c["account"] = r("ClaudeAccount", a);
    }
    c
}

fn cases() -> Vec<Case> {
    let c = |what, principal, action, resource, context, allow| Case { what, principal, action, resource, context, allow };
    let none = || json!({});
    vec![
        // The owner, an admin, through their phone.
        c("admin reads another person's session", ("Client", "ank-phone"), "StreamSession", ("Session", "s-bea-box"), none(), true),
        c("admin edits policies", ("Person", "ank"), "ManagePolicies", ("Hub", "hub"), none(), true),
        c("admin resolves an app's request", ("Client", "ank-phone"), "Resolve", ("Session", "s-rev-1"), none(), true),
        c("admin cannot kill a session labelled keep", ("Client", "ank-phone"), "Kill", ("Session", "s-rev-1"), none(), false),
        c("admin kills an unlabelled session", ("Client", "ank-phone"), "Kill", ("Session", "s-ank-rogue"), none(), true),
        // Another person.
        c("member reads own session", ("Person", "bea"), "StreamSession", ("Session", "s-bea-ghost"), none(), true),
        c("member's phone prompts own session", ("Client", "bea-phone"), "Prompt", ("Session", "s-bea-ghost"), none(), true),
        c("member reads her app's session", ("Client", "bea-phone"), "StreamSession", ("Session", "s-builder"), none(), true),
        c("member cannot read the owner's session", ("Person", "bea"), "StreamSession", ("Session", "s-ank-rogue"), none(), false),
        c("member cannot read the owner's bare session", ("Client", "bea-phone"), "ViewSession", ("Session", "s-bare-rogue"), none(), false),
        c("member sees a node shared with her", ("Client", "bea-phone"), "ViewNode", ("Node", "ghost001"), none(), true),
        c("member does not see an unshared node", ("Client", "bea-phone"), "ViewNode", ("Node", "rogue"), none(), false),
        c("member spawns on a shared node under a shared account", ("Client", "bea-phone"), "Spawn", ("Node", "ghost001"), spawn_ctx("ghost001/host", Some("bodygram")), true),
        c("member cannot spawn under an unshared account", ("Client", "bea-phone"), "Spawn", ("Node", "ghost001"), spawn_ctx("ghost001/host", Some("mrclmb")), false),
        c("member cannot spawn under a shared node's own login", ("Client", "bea-phone"), "Spawn", ("Node", "ghost001"), spawn_ctx("ghost001/host", None), false),
        c("member spawns on own node under its own login", ("Client", "bea-phone"), "Spawn", ("Node", "bea-box"), spawn_ctx("bea-box/host", None), true),
        c("member cannot spawn with another node's profile", ("Client", "bea-phone"), "Spawn", ("Node", "ghost001"), spawn_ctx("rogue/host", Some("bodygram")), false),
        c("member cannot spawn on an unshared node", ("Client", "bea-phone"), "Spawn", ("Node", "rogue"), spawn_ctx("rogue/host", Some("bodygram")), false),
        c("member has no terminal on a shared node", ("Client", "bea-phone"), "Terminal", ("Node", "ghost001"), none(), false),
        c("member has a terminal on her own node", ("Client", "bea-phone"), "Terminal", ("Node", "bea-box"), none(), true),
        c("member cannot kill the owner's shell", ("Client", "bea-phone"), "KillShell", ("Shell", "sh-rogue-1"), none(), false),
        c("member sees a shared account", ("Person", "bea"), "ViewAccount", ("ClaudeAccount", "bodygram"), none(), true),
        c("member cannot set a shared account's model", ("Person", "bea"), "SetClaudeTokenModel", ("ClaudeAccount", "bodygram"), none(), false),
        c("member sets her own account's model", ("Person", "bea"), "SetClaudeTokenModel", ("ClaudeAccount", "bea-acct"), none(), true),
        c("member cannot edit policies", ("Person", "bea"), "ManagePolicies", ("Hub", "hub"), none(), false),
        c("member manages her own clients", ("Client", "bea-phone"), "ManageClients", ("Person", "bea"), none(), true),
        c("member cannot manage another's clients", ("Person", "bea"), "ManageClients", ("Person", "cy"), none(), false),
        c("a person with nothing sees nothing", ("Person", "cy"), "ViewSession", ("Session", "s-bea-box"), none(), false),
        // Apps.
        c("app reads the session it launched", ("App", "reviewer"), "StreamSession", ("Session", "s-rev-1"), none(), true),
        c("app resolves its own session's request", ("App", "reviewer"), "Resolve", ("Session", "s-rev-1"), none(), true),
        c("app cannot read its owner's other session", ("App", "reviewer"), "StreamSession", ("Session", "s-ank-rogue"), none(), false),
        c("app cannot prompt another app's session", ("App", "reviewer"), "Prompt", ("Session", "s-builder"), none(), false),
        c("app spawns within its grant", ("App", "reviewer"), "Spawn", ("Node", "rogue"), spawn_ctx("rogue/host", Some("mrclmb")), true),
        c("app spawns on its embedded node", ("App", "reviewer"), "Spawn", ("Node", "reviewer-node"), spawn_ctx("reviewer-node/host", Some("mrclmb")), true),
        c("app cannot use an ungranted profile", ("App", "reviewer"), "Spawn", ("Node", "rogue"), spawn_ctx("rogue/sandbox", Some("mrclmb")), false),
        c("app cannot use an ungranted account", ("App", "reviewer"), "Spawn", ("Node", "rogue"), spawn_ctx("rogue/host", Some("bodygram")), false),
        c("app cannot spawn under a node's own login", ("App", "reviewer"), "Spawn", ("Node", "rogue"), spawn_ctx("rogue/host", None), false),
        c("app cannot spawn on an ungranted node", ("App", "reviewer"), "Spawn", ("Node", "ghost001"), spawn_ctx("ghost001/host", Some("mrclmb")), false),
        c("app resumes its session under a granted account", ("App", "reviewer"), "Resume", ("Session", "s-rev-1"), json!({"account": r("ClaudeAccount", "mrclmb")}), true),
        c("app cannot resume under the node's own login", ("App", "reviewer"), "Resume", ("Session", "s-rev-1"), json!({}), false),
        c("app sees a granted node", ("App", "reviewer"), "ViewNode", ("Node", "rogue"), none(), true),
        c("app does not see an ungranted node", ("App", "reviewer"), "ViewNode", ("Node", "ghost001"), none(), false),
        c("app has no terminal without the grant", ("App", "reviewer"), "Terminal", ("Node", "rogue"), none(), false),
        c("app with the grant has a terminal", ("App", "builder"), "Terminal", ("Node", "bea-box"), none(), true),
        c("app with the grant cannot kill a shell elsewhere", ("App", "builder"), "KillShell", ("Shell", "sh-rogue-1"), none(), false),
        c("app cannot set an account's model", ("App", "reviewer"), "SetClaudeTokenModel", ("ClaudeAccount", "mrclmb"), none(), false),
        c("app cannot manage tokens", ("App", "reviewer"), "ManageTokens", ("Hub", "hub"), none(), false),
        // A monitor (an app in the monitor role).
        c("monitor reads any session", ("App", "dash"), "StreamSession", ("Session", "s-bea-box"), none(), true),
        c("monitor sees any node", ("App", "dash"), "GetHealth", ("Node", "rogue"), none(), true),
        c("monitor cannot prompt", ("App", "dash"), "Prompt", ("Session", "s-bea-box"), none(), false),
        c("monitor has no terminal", ("App", "dash"), "Terminal", ("Node", "rogue"), none(), false),
        c("monitor cannot read the audit", ("App", "dash"), "ViewAudit", ("Hub", "hub"), none(), false),
        // Nodes.
        c("node gets its owner's account", ("Node", "rogue"), "GetClaudeToken", ("ClaudeAccount", "mrclmb"), none(), true),
        c("node gets an account shared with its owner", ("Node", "bea-box"), "GetClaudeToken", ("ClaudeAccount", "bodygram"), none(), true),
        c("node cannot get another person's account", ("Node", "bea-box"), "GetClaudeToken", ("ClaudeAccount", "mrclmb"), none(), false),
        c("embedded node gets its app's account", ("Node", "reviewer-node"), "UseClaudeApi", ("ClaudeAccount", "mrclmb"), none(), true),
        c("embedded node cannot get its owner's other account", ("Node", "reviewer-node"), "GetClaudeToken", ("ClaudeAccount", "bodygram"), none(), false),
        c("node reports its own session", ("Node", "rogue"), "ReportSession", ("Session", "s-ank-rogue"), none(), true),
        c("node cannot report another node's session", ("Node", "rogue"), "ReportSession", ("Session", "s-bea-box"), none(), false),
    ]
}

fn request(schema: &Schema, p: (&str, &str), action: &str, res: (&str, &str), ctx: &Value) -> Result<Request> {
    let action = EntityUid::from_str(&format!("Saarthi::Action::\"{action}\""))?;
    let context = Context::from_json_value(ctx.clone(), Some((schema, &action)))
        .map_err(|e| anyhow::anyhow!("context: {e}"))?;
    Request::new(uid(p.0, p.1), action, uid(res.0, res.1), context, Some(schema))
        .map_err(|e| anyhow::anyhow!("request: {e}"))
}

fn check() -> Result<()> {
    let schema = schema()?;
    // H1: every policy file validates against the schema in strict mode.
    let validator = Validator::new(schema.clone());
    let all = policies(&[])?;
    let v = validator.validate(&all, ValidationMode::Strict);
    let mut errors = 0;
    for e in v.validation_errors() {
        println!("validation error: {e}");
        errors += 1;
    }
    for w in v.validation_warnings() {
        println!("validation warning: {w}");
    }
    let entities = Entities::from_json_value(Value::Array(world()), Some(&schema))
        .map_err(|e| anyhow::anyhow!("entities: {e}"))?;
    let auth = Authorizer::new();
    // H2: the table.
    let mut wrong = 0;
    let cases = cases();
    for c in &cases {
        let req = request(&schema, c.principal, c.action, c.resource, &c.context)
            .with_context(|| c.what.to_string())?;
        let resp = auth.is_authorized(&req, &all, &entities);
        let allow = resp.decision() == Decision::Allow;
        let reasons: Vec<String> = resp
            .diagnostics()
            .reason()
            .map(|p| all.policy(p).and_then(|p| p.annotation("id")).unwrap_or("?").to_string())
            .collect();
        let errs: Vec<String> = resp.diagnostics().errors().map(|e| e.to_string()).collect();
        let mark = if allow == c.allow { "ok " } else { wrong += 1; "BAD" };
        println!(
            "{mark} {:5} {:52} by {:?}{}",
            if allow { "allow" } else { "deny" },
            c.what,
            reasons,
            if errs.is_empty() { String::new() } else { format!(" errors {errs:?}") }
        );
    }
    // A request outside the schema (a node streaming a session) is refused before Cedar decides.
    let outside = request(&schema, ("Node", "rogue"), "StreamSession", ("Session", "s-ank-rogue"), &json!({})).is_err();
    println!("request outside the schema refused at construction: {outside}");
    // H4: a policy set swapped at run time applies on the next call.
    let live = Arc::new(RwLock::new(all));
    let probe = request(&schema, ("Person", "bea"), "StreamSession", ("Session", "s-bea-ghost"), &json!({}))?;
    let before = auth.is_authorized(&probe, &live.read().unwrap(), &entities).decision();
    *live.write().unwrap() = policies(&["members"])?;
    let during = auth.is_authorized(&probe, &live.read().unwrap(), &entities).decision();
    *live.write().unwrap() = policies(&[])?;
    let after = auth.is_authorized(&probe, &live.read().unwrap(), &entities).decision();
    let swap_ok = before == Decision::Allow && during == Decision::Deny && after == Decision::Allow;
    println!("swap: {before:?} -> without members {during:?} -> restored {after:?}");
    let policies_n = live.read().unwrap().policies().count();
    println!(
        "===RESULT=== {}",
        json!({"cases": cases.len(), "wrong": wrong, "validation_errors": errors,
               "policies": policies_n, "outside_schema_refused": outside, "swap_ok": swap_ok})
    );
    if wrong > 0 || errors > 0 || !swap_ok || !outside {
        bail!("{wrong} wrong, {errors} validation errors, swap {swap_ok}, outside {outside}");
    }
    Ok(())
}

/// A hub-sized world: `people` people with a phone each, `apps` apps, `nodes` nodes,
/// `sessions` sessions spread over them, a third launched by apps.
fn big_world(people: usize, apps: usize, nodes: usize, sessions: usize) -> Vec<Value> {
    let mut v = vec![
        ent("Role", "admin", json!({}), &[]),
        ent("Role", "member", json!({}), &[]),
        ent("Role", "monitor", json!({}), &[]),
    ];
    let p = |i: usize| format!("p{i}");
    for i in 0..people {
        let role = if i == 0 { "admin" } else { "member" };
        v.push(ent("Person", &p(i), json!({"name": p(i)}), &[("Role", role)]));
        v.push(ent("Client", &format!("phone{i}"), json!({"kind": "device"}), &[("Person", &p(i))]));
        v.push(ent("ClaudeAccount", &format!("acct{i}"), json!({"owner": r("Person", &p(i)), "shared_with": []}), &[]));
    }
    for n in 0..nodes {
        let owner = p(n % people);
        let shared: Vec<Value> = (0..3).map(|k| r("Person", &p((n + k + 1) % people))).collect();
        v.push(ent("Node", &format!("n{n}"), json!({"owner": r("Person", &owner), "shared_with": shared}), &[]));
        v.push(ent("Profile", &format!("n{n}/host"), json!({"node": r("Node", &format!("n{n}")), "sandboxed": false}), &[]));
    }
    for a in 0..apps {
        let owner = a % people;
        let nodes_g: Vec<Value> = (0..4).map(|k| r("Node", &format!("n{}", (a * 4 + k) % nodes))).collect();
        let profiles_g: Vec<Value> = (0..4).map(|k| r("Profile", &format!("n{}/host", (a * 4 + k) % nodes))).collect();
        v.push(ent("App", &format!("app{a}"), json!({"owner": r("Person", &p(owner)),
            "grant": {"nodes": nodes_g, "profiles": profiles_g, "accounts": [r("ClaudeAccount", &format!("acct{owner}"))], "terminal": false}}), &[]));
    }
    for s in 0..sessions {
        let node = s % nodes;
        let (owner, app) = if s % 3 == 0 { let a = s % apps; (a % people, Some(format!("app{a}"))) } else { (s % people, None) };
        let mut e = session(&format!("s{s}"), &format!("n{node}"), &p(owner), "x", app.as_deref(), json!({"job": s.to_string()}));
        if app.is_none() {
            e["attrs"].as_object_mut().unwrap().remove("app");
        }
        v.push(e);
    }
    v
}

fn pct(sorted: &[f64], q: f64) -> f64 {
    sorted[((sorted.len() as f64 - 1.0) * q).round() as usize]
}

fn bench() -> Result<()> {
    let schema = schema()?;
    let all = policies(&[])?;
    let auth = Authorizer::new();
    let (people, apps, nodes) = (10, 40, 20);
    let mut results = serde_json::Map::new();
    for &n in &[100usize, 1_000, 10_000] {
        let world = big_world(people, apps, nodes, n);
        let t = Instant::now();
        let entities = Entities::from_json_value(Value::Array(world.clone()), Some(&schema))
            .map_err(|e| anyhow::anyhow!("entities: {e}"))?;
        let build_ms = t.elapsed().as_secs_f64() * 1e3;

        // One call: a random principal (a phone or an app), a read or operate action, a session.
        let actions = ["StreamSession", "Prompt", "Kill", "ViewSession", "Resolve"];
        let mut lat = Vec::new();
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut next = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
        let mut allowed = 0;
        for _ in 0..20_000 {
            let x = next() as usize;
            let principal = if x % 2 == 0 { ("Client", format!("phone{}", x % people)) } else { ("App", format!("app{}", x % apps)) };
            let req = request(&schema, (principal.0, &principal.1), actions[x % actions.len()],
                ("Session", &format!("s{}", (x >> 8) % n)), &json!({}))?;
            let t = Instant::now();
            let d = auth.is_authorized(&req, &all, &entities).decision();
            lat.push(t.elapsed().as_secs_f64() * 1e6);
            if d == Decision::Allow { allowed += 1; }
        }
        lat.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // Filtering a list: one principal, ViewSession on every session (requests built ahead).
        let reqs: Vec<Request> = (0..n)
            .map(|s| request(&schema, ("App", "app1"), "ViewSession", ("Session", &format!("s{s}")), &json!({})))
            .collect::<Result<_>>()?;
        let t = Instant::now();
        let visible = reqs.iter().filter(|q| auth.is_authorized(q, &all, &entities).decision() == Decision::Allow).count();
        let filter_ms = t.elapsed().as_secs_f64() * 1e3;

        // Per call with only the entities the request touches (principal and its ancestors, the
        // session, its node and app, the app's grant), built from JSON each time.
        let slice_ids: HashSet<String> = ["Saarthi::Role::admin", "Saarthi::Role::member", "Saarthi::Role::monitor",
            "Saarthi::App::app1", "Saarthi::Person::p1", "Saarthi::Session::s7", "Saarthi::Node::n7",
            "Saarthi::Node::n4", "Saarthi::Node::n5", "Saarthi::Node::n6", "Saarthi::Profile::n4/host",
            "Saarthi::Profile::n5/host", "Saarthi::Profile::n6/host", "Saarthi::Profile::n7/host",
            "Saarthi::ClaudeAccount::acct1", "Saarthi::App::app7"].iter().map(|s| s.to_string()).collect();
        let slice: Vec<Value> = world.iter().filter(|e| {
            let id = format!("{}::{}", e["uid"]["type"].as_str().unwrap(), e["uid"]["id"].as_str().unwrap());
            slice_ids.contains(&id)
        }).cloned().collect();
        let req = request(&schema, ("App", "app1"), "StreamSession", ("Session", "s7"), &json!({}))?;
        let mut slat = Vec::new();
        for _ in 0..2_000 {
            let t = Instant::now();
            let ents = Entities::from_json_value(Value::Array(slice.clone()), Some(&schema))
                .map_err(|e| anyhow::anyhow!("slice: {e}"))?;
            let _ = auth.is_authorized(&req, &all, &ents);
            slat.push(t.elapsed().as_secs_f64() * 1e6);
        }
        slat.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // A session changing (its labels, its owner) in the hub's live set: one entity
        // upserted into all of them, the set cloned first as a hub holding it shared would.
        let mut ulat = Vec::new();
        let mut live = entities.clone();
        for i in 0..200 {
            let mut e = session(&format!("s{}", i % n), "n1", "p1", "x", None, json!({"job": i.to_string()}));
            e["attrs"].as_object_mut().unwrap().remove("app");
            let t = Instant::now();
            let one = cedar_policy::Entity::from_json_value(e, Some(&schema))
                .map_err(|e| anyhow::anyhow!("entity: {e}"))?;
            live = live.clone().upsert_entities([one], Some(&schema))
                .map_err(|e| anyhow::anyhow!("upsert: {e}"))?;
            ulat.push(t.elapsed().as_secs_f64() * 1e3);
        }
        ulat.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let upsert_ms = pct(&ulat, 0.5);

        println!(
            "sessions {n:>6}: one session upserted into the live set p50 {upsert_ms:.2} ms p99 {:.2} ms",
            pct(&ulat, 0.99)
        );
        println!(
            "sessions {n:>6}: entities built in {build_ms:.1} ms; one call p50 {:.1} µs p99 {:.1} µs max {:.1} µs ({allowed} of 20000 allowed); \
             filtering {n} sessions for an app {filter_ms:.2} ms ({visible} visible); per call with a {}-entity slice p50 {:.1} µs p99 {:.1} µs",
            pct(&lat, 0.5), pct(&lat, 0.99), lat[lat.len() - 1], slice.len(), pct(&slat, 0.5), pct(&slat, 0.99)
        );
        results.insert(format!("s{n}"), json!({
            "entities_build_ms": (build_ms * 100.0).round() / 100.0,
            "call_p50_us": (pct(&lat, 0.5) * 10.0).round() / 10.0,
            "call_p99_us": (pct(&lat, 0.99) * 10.0).round() / 10.0,
            "filter_ms": (filter_ms * 100.0).round() / 100.0,
            "slice_call_p99_us": (pct(&slat, 0.99) * 10.0).round() / 10.0,
            "upsert_p50_ms": (upsert_ms * 100.0).round() / 100.0,
        }));
    }
    println!("===RESULT=== {}", Value::Object(results));
    Ok(())
}

fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("check") => check(),
        Some("bench") => bench(),
        _ => bail!("usage: cedar-spike check | bench"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_table_decides_as_intended() {
        super::check().unwrap();
    }
}
