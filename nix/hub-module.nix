# The saarthi hub as a NixOS module (ADR-0041).
#
# The hub records every node's sessions, events and health over one gRPC stream per node,
# and relays a client's decisions back. Nodes and clients (the app, `saarthi ctl`) connect
# to it. Auth is pre-shared tokens in two files of `<name> <token>` lines, one for nodes,
# one for clients; keep them in a secrets manager (agenix, sops-nix) readable by the
# service user. The SQLite database under `dataDir` is the hub's record: put it on storage
# that survives reprovisioning.
#
# `self` is this flake, for the default package.
self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.saarthi-hub;
  port = lib.toInt (lib.last (lib.splitString ":" cfg.listen));
in
{
  options.services.saarthi-hub = {
    enable = lib.mkEnableOption "the saarthi hub";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.saarthi-hub;
      defaultText = lib.literalExpression "saarthi.packages.\${system}.saarthi-hub";
      description = "The saarthi-hub package to run.";
    };

    listen = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0:7700";
      example = "100.64.0.1:7700";
      description = ''
        Address nodes and clients connect to. Keep the port off the public internet: leave
        `openFirewall` false and reach it over a private network such as a tailnet, or
        bind that network's address.
      '';
    };

    dataDir = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/saarthi-hub";
      example = "/srv/saarthi";
      description = "Directory of the SQLite database, `hub.db`, created on first start.";
    };

    nodeTokensFile = lib.mkOption {
      type = lib.types.path;
      description = "File of `<node name> <token>` lines, readable by the service user.";
    };

    clientTokensFile = lib.mkOption {
      type = lib.types.path;
      description = "File of `<client name> <token>` lines, readable by the service user.";
    };

    ntfy = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "http://127.0.0.1:2586/saarthi";
      description = ''
        An ntfy topic URL the hub posts to when a session waits on a person or finishes a
        turn. Registered UnifiedPush endpoints are notified regardless. Null: no topic.
      '';
    };

    noNotify = lib.mkOption {
      type = lib.types.listOf (
        lib.types.enum [
          "permission"
          "question"
          "turn"
          "ended"
          "usage"
        ]
      );
      default = [ ];
      description = "Notification kinds not to send (ADR-0029, ADR-0032).";
    };

    extraArgs = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = "Extra arguments appended to `saarthi-hub`.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "saarthi-hub";
      description = "System user the hub runs as; it owns the data directory.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "saarthi-hub";
      description = "Primary group of the service user.";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open `listen`'s port in the host firewall.";
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.${cfg.user} = {
      isSystemUser = true;
      group = cfg.group;
      home = cfg.dataDir;
      description = "saarthi hub service user";
    };
    users.groups.${cfg.group} = { };

    systemd.tmpfiles.rules = [ "d ${cfg.dataDir} 0750 ${cfg.user} ${cfg.group} - -" ];

    systemd.services.saarthi-hub = {
      description = "saarthi hub: records nodes' sessions, events and health, relays decisions";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      serviceConfig = {
        ExecStart = lib.escapeShellArgs (
          [
            (lib.getExe cfg.package)
            "--listen"
            cfg.listen
            "--db"
            "${cfg.dataDir}/hub.db"
            "--node-tokens"
            (toString cfg.nodeTokensFile)
            "--client-tokens"
            (toString cfg.clientTokensFile)
          ]
          ++ lib.optionals (cfg.ntfy != null) [
            "--ntfy"
            cfg.ntfy
          ]
          ++ lib.concatMap (k: [
            "--no-notify"
            k
          ]) cfg.noNotify
          ++ cfg.extraArgs
        );
        User = cfg.user;
        Group = cfg.group;
        Restart = "on-failure";
        RestartSec = 5;
        # The hub needs a TCP listener, its data directory and the token files.
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ReadWritePaths = [ cfg.dataDir ];
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
        ];
        RestrictNamespaces = true;
        LockPersonality = true;
      };
    };

    networking.firewall = lib.mkIf cfg.openFirewall { allowedTCPPorts = [ port ]; };
  };
}
