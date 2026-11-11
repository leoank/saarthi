# Evaluation tests of the hub and node modules, run as `checks.modules`. The hub is
# evaluated in a real NixOS system; the node against stubs of the four home-manager
# options it sets, so the flake needs no home-manager input. Both platforms of the node
# are evaluated wherever the check runs. A failing test fails evaluation and names itself.
{
  pkgs,
  lib,
  nixpkgs,
  self,
}:
let
  fake = name: pkgs.writeShellScriptBin name "exit 0";

  # ---- hub ----
  hub =
    extra:
    (nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        self.nixosModules.saarthi-hub
        {
          system.stateVersion = "26.05";
          boot.loader.grub.enable = false;
          fileSystems."/" = {
            device = "none";
            fsType = "tmpfs";
          };
          services.saarthi-hub = {
            enable = true;
            package = fake "saarthi-hub";
            nodeTokensFile = "/run/agenix/n";
            clientTokensFile = "/run/agenix/c";
          }
          // extra;
        }
      ];
    }).config;
  hubBase = hub { };
  hubFull = hub {
    ntfy = "http://127.0.0.1:2586/saarthi";
    claudeTokensFile = "/run/agenix/k";
    noNotify = [ "turn" ];
    dataDir = "/srv/saarthi";
    openFirewall = true;
    extraArgs = [ "--x" ];
  };
  exec = c: c.systemd.services.saarthi-hub.serviceConfig.ExecStart;

  hubTests = {
    test-hub-defaults = {
      expr =
        let
          s = hubBase.systemd.services.saarthi-hub;
          e = exec hubBase;
        in
        {
          wantedBy = s.wantedBy;
          user = s.serviceConfig.User;
          db = lib.hasInfix "--db /var/lib/saarthi-hub/hub.db" e;
          tokens = lib.hasInfix "--node-tokens /run/agenix/n --client-tokens /run/agenix/c" e;
          noNtfy = !(lib.hasInfix "--ntfy" e);
          noClaudeTokens = !(lib.hasInfix "--claude-tokens" e);
          systemUser = hubBase.users.users.saarthi-hub.isSystemUser;
          rw = s.serviceConfig.ReadWritePaths;
          firewallClosed = !(builtins.elem 7700 hubBase.networking.firewall.allowedTCPPorts);
        };
      expected = {
        wantedBy = [ "multi-user.target" ];
        user = "saarthi-hub";
        db = true;
        tokens = true;
        noNtfy = true;
        noClaudeTokens = true;
        systemUser = true;
        rw = [ "/var/lib/saarthi-hub" ];
        firewallClosed = true;
      };
    };
    test-hub-options = {
      expr =
        let
          e = exec hubFull;
        in
        {
          tail = lib.hasSuffix "--claude-tokens /run/agenix/k --ntfy http://127.0.0.1:2586/saarthi --no-notify turn --x" e;
          db = lib.hasInfix "--db /srv/saarthi/hub.db" e;
          port = builtins.elem 7700 hubFull.networking.firewall.allowedTCPPorts;
        };
      expected = {
        tail = true;
        db = true;
        port = true;
      };
    };
  };

  # ---- node ----
  stubHm =
    { lib, ... }:
    let
      anything = lib.mkOption {
        type = lib.types.attrsOf lib.types.anything;
        default = { };
      };
    in
    {
      options = {
        home.packages = lib.mkOption {
          type = lib.types.listOf lib.types.package;
          default = [ ];
        };
        programs.claude-code.settings = anything;
        launchd.agents = anything;
        systemd.user.services = anything;
      };
    };
  node =
    system: extra:
    let
      p = import nixpkgs { inherit system; };
      pkg = p.writeShellScriptBin "saarthi" "exit 0";
    in
    {
      bin = lib.getExe pkg;
      inherit pkg;
      config =
        (lib.evalModules {
          modules = [
            stubHm
            self.homeModules.saarthi-node
            {
              _module.args.pkgs = p;
              services.saarthi-node = {
                enable = true;
                package = pkg;
                hub = "http://hub:7700";
                tokenFile = "/run/agenix/t";
              }
              // extra;
            }
          ];
        }).config;
    };
  line =
    n:
    let
      c = n.config;
    in
    if c.launchd.agents ? saarthi-node then
      lib.last c.launchd.agents.saarthi-node.config.ProgramArguments
    else
      c.systemd.user.services.saarthi-node.Service.ExecStart;
  cmd = bin: "${bin} node serve --hub http://hub:7700 --token-file /run/agenix/t --hook-addr 127.0.0.1:7777";
  full = {
    name = "rogue";
    ignoreCwd = [ "*/target/live-work-*" ];
    claudeUsage = true;
    keepAwake = true;
    spawnRoots = [
      "/home/u/a"
      "/home/u/b"
    ];
    claude = "/opt/claude";
    profiles.sandbox.backend = "seatbelt";
    extraArgs = [
      "--hold-secs"
      "60"
    ];
  };

  nodeTests = lib.concatMapAttrs (
    tag: system:
    let
      base = node system { };
      rich = node system full;
      isDarwin = lib.hasSuffix "darwin" system;
      l = line rich;
    in
    {
      "test-node-${tag}-service" = {
        expr =
          if isDarwin then
            let
              a = base.config.launchd.agents.saarthi-node;
            in
            {
              on = a.enable;
              cmd = line base == cmd base.bin;
              keep = a.config.KeepAlive;
              linux = base.config.systemd.user.services ? saarthi-node;
            }
          else
            let
              s = base.config.systemd.user.services.saarthi-node;
            in
            {
              on = s.Service.Restart == "on-failure";
              cmd = lib.hasSuffix (lib.escapeShellArg (cmd base.bin)) s.Service.ExecStart;
              keep = s.Install.WantedBy == [ "default.target" ];
              darwin = base.config.launchd.agents ? saarthi-node;
            };
        expected = {
          on = true;
          cmd = true;
          keep = true;
        }
        // (if isDarwin then { linux = false; } else { darwin = false; });
      };
      "test-node-${tag}-flags" = {
        expr = {
          off = builtins.any (f: lib.hasInfix f (line base)) [
            "--spawn-root"
            "--claude "
            "--profiles"
          ];
          name = lib.hasInfix "--name rogue" l;
          ignore = lib.hasInfix "--ignore-cwd" l && lib.hasInfix "live-work-" l;
          usage = lib.hasInfix "--claude-usage --keep-awake --spawn-root /home/u/a --spawn-root /home/u/b --claude /opt/claude" l;
          profiles =
            builtins.match ".*--profiles /nix/store/[a-z0-9]+-saarthi-profiles\\.toml --hold-secs 60.*" l != null;
          onPath = rich.config.home.packages == [ rich.pkg ];
        };
        expected = {
          off = false;
          name = true;
          ignore = true;
          usage = true;
          profiles = true;
          onPath = true;
        };
      };
    }
  ) {
    darwin = "aarch64-darwin";
    linux = "x86_64-linux";
  };

  nodeHooks =
    let
      h = (node "x86_64-linux" { }).config.programs.claude-code.settings.hooks;
      off = (node "x86_64-linux" { claudeHooks = false; }).config.programs.claude-code.settings;
    in
    {
      test-node-hooks = {
        expr = {
          events = builtins.attrNames h;
          stop = h.Stop;
          off = off ? hooks;
        };
        expected = {
          events = [
            "Notification"
            "PermissionRequest"
            "PostToolUse"
            "PreToolUse"
            "SessionEnd"
            "SessionStart"
            "Stop"
            "SubagentStop"
            "UserPromptSubmit"
          ];
          stop = [
            {
              hooks = [
                {
                  type = "http";
                  url = "http://127.0.0.1:7777/hook";
                  timeout = 600;
                }
              ];
            }
          ];
          off = false;
        };
      };
    };

  failures = lib.runTests (hubTests // nodeTests // nodeHooks);
in
if failures == [ ] then
  pkgs.runCommand "saarthi-modules" { } "touch $out"
else
  throw "module tests failed: ${builtins.toJSON failures}"
