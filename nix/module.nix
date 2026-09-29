{
  config,
  lib,
  pkgs,
  ...
}:

with lib;

let
  cfg = config.services.home-registry;
  isDefaultDataDir = cfg.dataDir == "/var/lib/home-registry";

  jwtSecretPath = "${cfg.dataDir}/data/jwt_secret";

  preStartScript = pkgs.writeShellScript "home-registry-prestart" ''
    set -euo pipefail
    mkdir -p uploads/img backups data
    ln -sfn ${cfg.package}/share/home-registry/static static
    if [ ! -s data/jwt_secret ]; then
      head -c48 /dev/urandom | base64 > data/jwt_secret
      chmod 600 data/jwt_secret
    fi
  '';

  dbPasswordSyncScript = pkgs.writeShellScript "home-registry-db-password" ''
    set -euo pipefail
    # shellcheck disable=SC1090
    source "${toString cfg.environmentFile}"
    if [ -z "''${POSTGRES_PASSWORD:-}" ]; then
      echo "home-registry: POSTGRES_PASSWORD not found in environmentFile" >&2
      exit 1
    fi
    ${pkgs.postgresql}/bin/psql -v ON_ERROR_STOP=1 -v pass="$POSTGRES_PASSWORD" \
      -c "ALTER ROLE ${cfg.database.user} WITH PASSWORD :'pass';"
  '';
in
{
  options.services.home-registry = {
    enable = mkEnableOption "the home-registry self-hosted inventory server";

    package = mkOption {
      type = types.package;
      default = pkgs.home-registry;
      defaultText = literalExpression "pkgs.home-registry";
      description = ''
        The home-registry package to run. Defaults to `pkgs.home-registry`, made
        available by importing `nixosModules.default` (which applies this flake's
        overlay automatically).
      '';
    };

    port = mkOption {
      type = types.port;
      default = 8210;
      description = "TCP port the HTTP server listens on.";
    };

    dataDir = mkOption {
      type = types.path;
      default = "/var/lib/home-registry";
      description = ''
        Directory holding the persisted JWT secret, uploaded images, backups, and a
        symlink to the packaged static frontend assets. When left at its default,
        the service runs with `DynamicUser` and a systemd-managed `StateDirectory`;
        overriding it switches the service to a dedicated static system user.
      '';
    };

    environmentFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        Path to an EnvironmentFile (outside the Nix store) providing secrets such as
        `POSTGRES_PASSWORD` and, optionally, `JWT_SECRET`. Required when
        `database.createLocally` is enabled.
      '';
    };

    openFirewall = mkOption {
      type = types.bool;
      default = true;
      description = "Whether to open the firewall for `port`.";
    };

    rateLimitRps = mkOption {
      type = types.nullOr types.int;
      default = null;
      description = "Optional override for RATE_LIMIT_RPS (requests/second).";
    };

    rateLimitBurst = mkOption {
      type = types.nullOr types.int;
      default = null;
      description = "Optional override for RATE_LIMIT_BURST.";
    };

    database = {
      createLocally = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Provision a local PostgreSQL database and role via
          `services.postgresql.ensureDatabases`/`ensureUsers`. Requires
          `environmentFile` to supply `POSTGRES_PASSWORD`, which is synced into the
          role on every service (re)start. When false, `database.host` is expected
          to point at an externally-managed PostgreSQL instance.
        '';
      };

      host = mkOption {
        type = types.str;
        default = "127.0.0.1";
        description = "PostgreSQL host (POSTGRES_HOST).";
      };

      port = mkOption {
        type = types.port;
        default = 5432;
        description = "PostgreSQL port (POSTGRES_PORT).";
      };

      user = mkOption {
        type = types.str;
        default = "home_registry";
        description = "PostgreSQL role (POSTGRES_USER).";
      };

      name = mkOption {
        type = types.str;
        default = "home_registry";
        description = "PostgreSQL database name (POSTGRES_DB).";
      };
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = !cfg.database.createLocally || cfg.environmentFile != null;
        message = ''
          services.home-registry.database.createLocally requires
          services.home-registry.environmentFile to be set (must provide
          POSTGRES_PASSWORD) so the local role's password can be synced.
        '';
      }
    ];

    users.users = mkIf (!isDefaultDataDir) {
      home-registry = {
        isSystemUser = true;
        group = "home-registry";
      };
    };
    users.groups = mkIf (!isDefaultDataDir) {
      home-registry = { };
    };

    systemd.tmpfiles.rules = mkIf (!isDefaultDataDir) [
      "d ${cfg.dataDir} 0750 home-registry home-registry - -"
    ];

    services.postgresql = mkIf cfg.database.createLocally {
      enable = true;
      ensureDatabases = [ cfg.database.name ];
      ensureUsers = [
        {
          name = cfg.database.user;
          ensureDBOwnership = true;
        }
      ];
      authentication = ''
        host  ${cfg.database.name}  ${cfg.database.user}  127.0.0.1/32  scram-sha-256
        host  ${cfg.database.name}  ${cfg.database.user}  ::1/128       scram-sha-256
      '';
    };

    systemd.services.home-registry-db-password = mkIf cfg.database.createLocally {
      description = "Sync home-registry PostgreSQL role password";
      after = [ "postgresql.service" ];
      before = [ "home-registry.service" ];
      wantedBy = [ "home-registry.service" ];
      serviceConfig = {
        Type = "oneshot";
        User = "postgres";
        ExecStart = "${dbPasswordSyncScript}";
      };
    };

    systemd.services.home-registry =
      let
        commonEnv = {
          PORT = toString cfg.port;
          POSTGRES_HOST = cfg.database.host;
          POSTGRES_PORT = toString cfg.database.port;
          POSTGRES_USER = cfg.database.user;
          POSTGRES_DB = cfg.database.name;
          JWT_SECRET_FILE = jwtSecretPath;
          RUST_LOG = "info";
        }
        // optionalAttrs (cfg.rateLimitRps != null) { RATE_LIMIT_RPS = toString cfg.rateLimitRps; }
        // optionalAttrs (cfg.rateLimitBurst != null) {
          RATE_LIMIT_BURST = toString cfg.rateLimitBurst;
        };
      in
      {
        description = "home-registry self-hosted inventory server";
        after = [ "network.target" ] ++ optional cfg.database.createLocally "postgresql.service";
        wants = optional cfg.database.createLocally "postgresql.service";
        wantedBy = [ "multi-user.target" ];

        environment = commonEnv;

        serviceConfig =
          {
            ExecStartPre = "${preStartScript}";
            ExecStart = "${cfg.package}/bin/home-registry";
            WorkingDirectory = cfg.dataDir;
            Restart = "on-failure";
            RestartSec = "5s";

            NoNewPrivileges = true;
            ProtectSystem = "strict";
            ProtectHome = true;
            PrivateTmp = true;
            ReadWritePaths = [ cfg.dataDir ];
          }
          // (
            if isDefaultDataDir then
              {
                DynamicUser = true;
                StateDirectory = "home-registry";
              }
            else
              {
                User = "home-registry";
                Group = "home-registry";
              }
          )
          // optionalAttrs (cfg.environmentFile != null) {
            EnvironmentFile = cfg.environmentFile;
          };
      };

    networking.firewall = mkIf cfg.openFirewall {
      allowedTCPPorts = [ cfg.port ];
    };
  };
}
