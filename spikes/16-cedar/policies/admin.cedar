// An admin (and their devices) may do anything a person can.
@id("admin")
permit (principal in Saarthi::Role::"admin", action, resource);
