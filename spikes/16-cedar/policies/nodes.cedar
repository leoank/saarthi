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
