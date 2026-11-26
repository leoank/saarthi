// Labels as tags: a session labelled keep=true is never killed or deleted, by anyone.
@id("keep")
forbid (
  principal,
  action in [Saarthi::Action::"Kill", Saarthi::Action::"DeleteSession"],
  resource is Saarthi::Session
)
when { resource.hasTag("keep") && resource.getTag("keep") == "true" };
