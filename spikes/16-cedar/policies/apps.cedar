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
