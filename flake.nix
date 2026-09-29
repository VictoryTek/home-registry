{
  description = "home-registry - self-hosted home inventory management system";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    let
      linuxSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      version = "0.1.0-beta.3";

      # Builds the combined package (Rust binary + synced frontend static assets)
      # against an arbitrary `pkgs` instance, so it can be used both per-system
      # (packages.<system>.default) and from the overlay (pkgs.home-registry),
      # without duplicating the derivation logic.
      mkHomeRegistry =
        pkgs:
        let
          lib = pkgs.lib;

          frontend = pkgs.buildNpmPackage {
            pname = "home-registry-frontend";
            inherit version;

            src = ./frontend;

            npmDepsHash = "sha256-J9g5OK/v8Qn8UD81kvpTDsSgBVkztpI4An57rxaSROU=";

            npmBuildScript = "build";

            installPhase = ''
              runHook preInstall
              mkdir -p $out
              cp -r dist/. $out/
              runHook postInstall
            '';
          };
        in
        pkgs.rustPlatform.buildRustPackage {
          pname = "home-registry";
          inherit version;

          src = lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              let
                baseName = baseNameOf path;
              in
              !(
                baseName == "frontend"
                || baseName == "target"
                || baseName == "node_modules"
                || baseName == ".github"
                || baseName == "nix"
              );
          };

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          # Pure-Rust dependency graph (NoTls, argon2/aes-gcm/sha2 are pure Rust) -
          # no extra native build inputs required.

          doCheck = false;

          postInstall = ''
            mkdir -p $out/share/home-registry
            cp -r ${frontend} $out/share/home-registry/static
          '';

          meta = with lib; {
            description = "Self-hosted home inventory management system";
            homepage = "https://github.com/your-org/home-registry";
            license = licenses.mit;
            mainProgram = "home-registry";
            platforms = linuxSystems;
          };
        };

      overlay = final: _prev: {
        home-registry = mkHomeRegistry final;
      };
    in
    flake-utils.lib.eachSystem linuxSystems (
      system:
      let
        pkgs = import nixpkgs { inherit system; overlays = [ overlay ]; };
      in
      {
        packages.default = pkgs.home-registry;

        checks.default = pkgs.home-registry;
        checks.module-eval =
          (nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              self.nixosModules.default
              {
                services.home-registry.enable = true;
                system.stateVersion = "24.05";
                # Minimal fs/boot stubs so `nixosSystem` evaluates standalone,
                # without importing a real hardware-configuration.nix.
                fileSystems."/" = {
                  device = "/dev/disk/by-label/nixos";
                  fsType = "ext4";
                };
                boot.loader.grub.device = "nodev";
              }
            ];
          }).config.system.build.toplevel;
      }
    )
    // {
      overlays.default = overlay;

      nixosModules.default =
        { ... }:
        {
          imports = [ ./nix/module.nix ];
          nixpkgs.overlays = [ overlay ];
        };
    };
}
