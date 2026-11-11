# The saarthi node daemon as a home-manager module (ADR-0041).
#
# Runs `saarthi node serve` for the logged-in user: it observes every Claude Code
# session on the machine through the hook endpoint, the transcript and Claude Code's
# session registry, holds one gRPC stream to the hub, and gates permission dialogs. A
# launchd user agent on Darwin, a systemd user service on Linux; it runs as the user
# because it reads that user's `~/.claude`.
#
# The hooks block that points Claude Code at the daemon must live in the user's own
# settings file (SessionStart fires only from there, not from a `--settings` file). With
# `claudeHooks` (on by default) the module adds it to `programs.claude-code.settings`, the
# block `saarthi node hooks` prints; a block merged into a home-manager-written
# settings.json by hand is lost at the next switch.
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
  cfg = config.services.saarthi-node;
  toml = pkgs.formats.toml { };
  profilesFile = toml.generate "saarthi-profiles.toml" { profiles = cfg.profiles; };
  args = [
    (lib.getExe cfg.package)
    "node"
    "serve"
    "--hub"
    cfg.hub
    "--token-file"
    cfg.tokenFile
    "--hook-addr"
    cfg.hookAddr
  ]
  ++ lib.optionals (cfg.name != null) [
    "--name"
    cfg.name
  ]
  ++ lib.concatMap (g: [
    "--ignore-cwd"
    g
  ]) cfg.ignoreCwd
  ++ lib.optional cfg.claudeUsage "--claude-usage"
  ++ lib.optional cfg.keepAwake "--keep-awake"
  ++ lib.concatMap (r: [
    "--spawn-root"
    r
  ]) cfg.spawnRoots
  ++ lib.optionals (cfg.claude != null) [
    "--claude"
    cfg.claude
  ]
  ++ lib.optionals (cfg.profiles != { }) [
    "--profiles"
    "${profilesFile}"
  ]
  ++ cfg.extraArgs;
  cmdline = lib.escapeShellArgs args;
in
{
  options.services.saarthi-node = {
    enable = lib.mkEnableOption "the saarthi node daemon";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.saarthi;
      defaultText = lib.literalExpression "saarthi.packages.\${system}.saarthi";
      description = ''
        The package providing the `saarthi` binary. It is also put on the user's PATH,
        for `saarthi run`, `saarthi node hooks` and `saarthi ctl`.
      '';
    };

    hub = lib.mkOption {
      type = lib.types.str;
      example = "http://hub:7700";
      description = "The hub's node service, `http://host:port`.";
    };

    tokenFile = lib.mkOption {
      type = lib.types.str;
      example = "/run/agenix/saarthiNodeToken";
      description = ''
        File holding this node's pre-shared hub token. On Darwin give a fixed path: launchd
        does not expand the per-user temporary directory agenix defaults to.
      '';
    };

    name = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Name this node registers under. Null: the hostname.";
    };

    hookAddr = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1:7777";
      description = "Address the hook endpoint listens on; the hooks block points here.";
    };

    ignoreCwd = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = [ "*/target/live-work-*" ];
      description = ''
        Globs over working directories whose sessions the daemon does not track at all: not
        listed, not tailed, their dialogs not held (ADR-0028).
      '';
    };

    claudeUsage = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Report the account's rate-limit utilisation as health, reading the OAuth access
        token from Claude Code's credential store, read only, and polling Anthropic's usage
        endpoint every five minutes (ADR-0030).
      '';
    };

    keepAwake = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Keep the machine out of idle sleep while a session is busy (`caffeinate -i` on
        Darwin), so a turn finishes and the hub link holds when the screen turns off
        (ADR-0031).
      '';
    };

    spawnRoots = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = lib.literalExpression ''[ "''${config.home.homeDirectory}/workspace" ]'';
      description = ''
        Absolute directories a client may start headless sessions in, each itself or
        anything below it (ADR-0034). Empty: spawning is off. Sessions start through the
        user's login shell, so they get a terminal's PATH.
      '';
    };

    claude = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "/etc/profiles/per-user/me/bin/claude";
      description = "The `claude` spawned sessions run. Null: `claude` as the login shell finds it.";
    };

    profiles = lib.mkOption {
      type = lib.types.attrsOf toml.type;
      default = { };
      example = lib.literalExpression ''
        {
          sandbox = {
            backend = "seatbelt";
            description = "Confined: the working directory, the nix store and git";
            state_dir = "~/.local/state/saarthi/sandbox";
            token_file = "~/.config/saarthi/sandbox-claude-token";
            path = [ "/etc/profiles/per-user/me/bin" ];
          };
        }
      '';
      description = ''
        Launch profiles a spawned session may run under, written to the TOML file
        `--profiles` reads (ADR-0038): each a `host` or `seatbelt` backend, with its state
        directory, the file holding a token from `claude setup-token`, `path`, `readonly`,
        `readwrite` and `denied` paths, `claude_files` linked from `~/.claude`, and `env`.
        `~/` is the user's home. Empty: no profiles.
      '';
    };

    claudeHooks = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Add the hooks block of `saarthi node hooks` to `programs.claude-code.settings`, so
        every session reports to the daemon and its dialogs can be answered from a client.
        Off: merge that block into `~/.claude/settings.json` yourself.
      '';
    };

    extraArgs = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = [
        "--hold-secs"
        "60"
      ];
      description = "Extra arguments appended to `saarthi node serve`.";
    };

    logDir = lib.mkOption {
      type = lib.types.str;
      default = "/tmp";
      description = ''
        Directory of the Darwin logs, `<logDir>/saarthi-node.{out,err}.log`. On Linux the
        output goes to the user journal: `journalctl --user -u saarthi-node`.
      '';
    };
  };

  config = lib.mkIf cfg.enable (
    lib.mkMerge [
      { home.packages = [ cfg.package ]; }

      # Every event the daemon handles, an HTTP hook to its endpoint. The 600 s timeout is
      # the default of `saarthi node hooks` and the one the daemon gives the sessions it
      # spawns, so Claude Code sees one hook, not two (ADR-0037).
      (lib.mkIf cfg.claudeHooks {
        programs.claude-code.settings.hooks =
          lib.genAttrs
            [
              "SessionStart"
              "UserPromptSubmit"
              "PreToolUse"
              "PermissionRequest"
              "PostToolUse"
              "Notification"
              "Stop"
              "SubagentStop"
              "SessionEnd"
            ]
            (_: [
              {
                hooks = [
                  {
                    type = "http";
                    url = "http://${cfg.hookAddr}/hook";
                    timeout = 600;
                  }
                ];
              }
            ]);
      })

      (lib.mkIf pkgs.stdenv.hostPlatform.isDarwin {
        launchd.agents.saarthi-node = {
          enable = true;
          config = {
            ProgramArguments = [
              "${pkgs.bash}/bin/bash"
              "-c"
              cmdline
            ];
            KeepAlive = true;
            RunAtLoad = true;
            StandardErrorPath = "${cfg.logDir}/saarthi-node.err.log";
            StandardOutPath = "${cfg.logDir}/saarthi-node.out.log";
          };
        };
      })

      (lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
        systemd.user.services.saarthi-node = {
          Unit.Description = "saarthi node daemon";
          Service = {
            ExecStart = "${pkgs.bash}/bin/bash -c ${lib.escapeShellArg cmdline}";
            Restart = "on-failure";
            RestartSec = 5;
          };
          Install.WantedBy = [ "default.target" ];
        };
      })
    ]
  );
}
