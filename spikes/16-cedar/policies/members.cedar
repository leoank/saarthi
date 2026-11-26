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
  (principal in context.account.owner || principal in context.account.shared_with)
};

// A person adds and revokes their own devices and apps.
@id("member-clients")
permit (principal, action == Saarthi::Action::"ManageClients", resource is Saarthi::Person)
when { principal in resource };
