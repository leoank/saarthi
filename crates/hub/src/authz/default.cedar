// The hub's default policies (ADR-0081): the bootstrap set it starts with. Every client in
// the client-token file is a device of the owner, who is an admin, and every node and
// Claude account is the owner's, so these allow what a client token always could.

// An admin (and their devices) may do anything a person can.
@id("admin")
permit (principal in Saarthi::Role::"admin", action, resource);

// Everyone's own: sessions they own, their nodes and the nodes shared with them, their
// accounts and those shared with them. `principal in X` holds for a person and their devices.
@id("member-sessions")
permit (
  principal,
  action in [Saarthi::Action::"read", Saarthi::Action::"operate"],
  resource is Saarthi::Session
)
when { principal in resource.owner };

@id("member-nodes-view")
permit (
  principal,
  action in [Saarthi::Action::"ViewNode", Saarthi::Action::"GetHealth"],
  resource is Saarthi::Node
)
when { principal in resource.owner || principal in resource.shared_with };

@id("member-nodes-own")
permit (
  principal,
  action in [Saarthi::Action::"SetKeepAwake", Saarthi::Action::"ServiceRequest", Saarthi::Action::"shell"],
  resource is Saarthi::Node
)
when { principal in resource.owner };

@id("member-shells")
permit (principal, action == Saarthi::Action::"KillShell", resource is Saarthi::Shell)
when { principal in resource.node.owner };

// A spawn on a node of theirs or shared with them, with a profile of that node, under an
// account of theirs or shared with them; the node's own login only on their own node.
@id("member-spawn")
permit (principal, action == Saarthi::Action::"Spawn", resource is Saarthi::Node)
when {
  (principal in resource.owner || principal in resource.shared_with) &&
  context.profile.node == resource &&
  (if context has account
   then principal in context.account.owner || principal in context.account.shared_with
   else principal in resource.owner)
};

@id("member-resume")
permit (principal, action == Saarthi::Action::"Resume", resource is Saarthi::Session)
when {
  principal in resource.owner &&
  (if context has account
   then principal in context.account.owner || principal in context.account.shared_with
   else principal in resource.node.owner)
};

@id("member-accounts")
permit (principal, action == Saarthi::Action::"ViewAccount", resource is Saarthi::ClaudeAccount)
when { principal in resource.owner || principal in resource.shared_with };

@id("member-accounts-manage")
permit (
  principal,
  action in [Saarthi::Action::"SetClaudeTokenExpiry", Saarthi::Action::"SetClaudeTokenModel"],
  resource is Saarthi::ClaudeAccount
)
when { principal in resource.owner };

@id("member-default-account")
permit (principal, action == Saarthi::Action::"SetDefaultClaudeToken", resource is Saarthi::Node)
when {
  principal in resource.owner &&
  (if context has account
   then principal in context.account.owner || principal in context.account.shared_with
   else true)
};

// A person adds and revokes their own devices and apps.
@id("member-clients")
permit (principal, action == Saarthi::Action::"ManageClients", resource is Saarthi::Person)
when { principal in resource };

// A monitor reads everything a client can list and nothing else.
@id("monitor")
permit (principal in Saarthi::Role::"monitor", action in Saarthi::Action::"read", resource);

// An app: the sessions it launched, and spawns within its grant, always under a granted
// account. No terminal unless granted, never the hub's admin or an account's settings.
@id("app-sessions")
permit (
  principal is Saarthi::App,
  action in [Saarthi::Action::"read", Saarthi::Action::"operate"],
  resource is Saarthi::Session
)
when { resource has app && resource.app == principal };

@id("app-resume-account")
forbid (principal is Saarthi::App, action == Saarthi::Action::"Resume", resource)
unless { context has account && principal.grant.accounts.contains(context.account) };

@id("app-nodes")
permit (
  principal is Saarthi::App,
  action in [Saarthi::Action::"ViewNode", Saarthi::Action::"GetHealth"],
  resource is Saarthi::Node
)
when { principal.grant.nodes.contains(resource) };

@id("app-spawn")
permit (principal is Saarthi::App, action == Saarthi::Action::"Spawn", resource is Saarthi::Node)
when {
  principal.grant.nodes.contains(resource) &&
  principal.grant.profiles.contains(context.profile) &&
  context.profile.node == resource &&
  context has account && principal.grant.accounts.contains(context.account)
};

@id("app-accounts")
permit (principal is Saarthi::App, action == Saarthi::Action::"ViewAccount", resource is Saarthi::ClaudeAccount)
when { principal.grant.accounts.contains(resource) };

@id("app-terminal")
permit (principal is Saarthi::App, action in [Saarthi::Action::"Terminal", Saarthi::Action::"ListShells"], resource is Saarthi::Node)
when { principal.grant.terminal && principal.grant.nodes.contains(resource) };

@id("app-kill-shell")
permit (principal is Saarthi::App, action == Saarthi::Action::"KillShell", resource is Saarthi::Shell)
when { principal.grant.terminal && principal.grant.nodes.contains(resource.node) };

// What a node may ask of the hub. A node embedded in an app gets only the app's accounts;
// another node its owner's and those shared with its owner.
@id("node-accounts")
permit (
  principal is Saarthi::Node,
  action in [Saarthi::Action::"GetClaudeToken", Saarthi::Action::"UseClaudeApi"],
  resource is Saarthi::ClaudeAccount
)
when {
  if principal has app
  then principal.app.grant.accounts.contains(resource)
  else principal.owner == resource.owner || resource.shared_with.contains(principal.owner)
};

@id("node-report")
permit (principal is Saarthi::Node, action == Saarthi::Action::"ReportSession", resource is Saarthi::Session)
when { resource.node == principal };

// Every principal holds its own push registration and notification stream.
@id("own-notifications")
permit (
  principal,
  action in [Saarthi::Action::"RegisterPush", Saarthi::Action::"Notifications"],
  resource is Saarthi::Hub
);
