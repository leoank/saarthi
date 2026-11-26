// A monitor reads everything a client can list and nothing else.
@id("monitor")
permit (principal in Saarthi::Role::"monitor", action in Saarthi::Action::"read", resource);
