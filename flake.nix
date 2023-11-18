{
  description = "Mail server for development purposes.";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    stable.url = "github:NixOS/nixpkgs/nixos-23.05";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    utils.url = "github:numtide/flake-utils";
    nix-filter.url = "github:numtide/nix-filter";
  };
  outputs = {
    self,
    nixpkgs,
    stable,
    utils,
    rust-overlay,
    nix-filter,
  }: let
    buildShell = pkgs: anyRustToolchain: let
      stablePkgs = import stable {
        inherit (pkgs) system;
      };
    in
      pkgs.mkShell {
        buildInputs = with pkgs; [
          anyRustToolchain
          # NOTE: Package is broken in unstable.
          stablePkgs.cargo-leptos
          wasm-pack
          binaryen
          leptosfmt
        ];
      };
    rustOverwrite = anyRustToolchain:
      anyRustToolchain.override {
        targets = ["wasm32-unknown-unknown"];
        extensions = ["rust-src" "rust-analyzer-preview"];
      };
    buildForSystem = system: let
      overlays = [self.overlays.combined];
      pkgs = import nixpkgs {
        inherit system overlays;
      };
    in {
      apps.default = {
        type = "app";
        program = "${self.packages.${system}.default}/bin/mail-blackhole";
      };
      packages = {
        default = pkgs.mail-blackhole;
      };
      devShells = rec {
        default = nightly;
        stable = buildShell pkgs (rustOverwrite pkgs.rust-bin.stable.latest.default);
        nightly = buildShell pkgs (rustOverwrite pkgs.rust-bin.nightly.latest.default);
      };
      legacyPackages = pkgs;
    };
  in
    (utils.lib.eachDefaultSystem buildForSystem)
    // {
      overlays = {
        default = self: super: let
          # NOTE: wasm-bindgen-cli requires a specific version due to
          # compatibility. Since the version might differ between the
          # nixpkgs's revisions, we pin them to a known good state and
          # inject these *binary* dependencies.
          pinnedPkgs = import nixpkgs {inherit (super) system;};

          rustc = rustOverwrite super.rust-bin.stable.latest.default;

          wasmPlatform = super.makeRustPlatform {
            cargo = rustc;
            rustc = rustc;
          };

          src = nix-filter.lib {
            root = ./.;
            include = [
              "src"
              "style"
              ./Cargo.toml
              ./Cargo.lock
            ];
          };

          manifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);

          leptos = manifest.package.metadata.leptos;
        in rec {
          # NOTE: We are using the wasmPlatform here because there are
          # features which required at least the 1.70.0
          # compiler. Currently the 23.05 Nixpkgs ship the 1.69.0
          # compiler.
          mail-blackhole = wasmPlatform.buildRustPackage {
            inherit src;

            pname = "mail-blackhole";
            version = manifest.package.version;

            "LEPTOS_OUTPUT_NAME" = leptos.output-name;
            "CARGO_BUNDLE_DIR" = "${mail-blackhole-web}/share/www";

            buildNoDefaultFeatures = true;
            buildFeatures = ["bundle"];

            cargoLock = {
              lockFile = ./Cargo.lock;
            };
          };
          mail-blackhole-web = wasmPlatform.buildRustPackage {
            inherit src;

            pname = "mail-blackhole-web";
            version = manifest.package.version;

            nativeBuildInputs = with pinnedPkgs; [
              wasm-bindgen-cli
              binaryen
              minify
            ];

            buildPhase = ''
              cargo build --lib --release --no-default-features --features "hydrate" --target wasm32-unknown-unknown
              echo "bindgen WASM"
              wasm-bindgen target/wasm32-unknown-unknown/release/mail_blackhole.wasm --out-dir . --web
              echo "optimize WASM"
              wasm-opt -Os -o mail_blackhole.wasm mail_blackhole_bg.wasm
            '';

            installPhase = ''
              mkdir -p $out/share/www
              cp mail_blackhole.wasm $out/share/www/${leptos.output-name}.wasm
              minify mail_blackhole.js > $out/share/www/${leptos.output-name}.js
              minify ${src}/style/style.css > $out/share/www/${leptos.output-name}.css
            '';

            cargoLock = {
              lockFile = ./Cargo.lock;
            };
          };
        };
        combined = nixpkgs.lib.composeManyExtensions [
          rust-overlay.overlays.default
          self.overlays.default
        ];
      };
      nixosModules.default = {
        config,
        lib,
        pkgs,
        ...
      }:
        with lib; let
          cfg = config.services.mail-blackhole;
        in {
          options.services.mail-blackhole = {
            enable = mkEnableOption "Mail Blackhole service";

            package = mkOption {
              type = types.package;
              default = pkgs.mail-blackhole;
              description = "Package used by this module";
            };

            listen-http = mkOption {
              type = types.str;
              description = "Listening address/port for http server.";
              default = "0.0.0.0:8080";
            };

            listen-mail = mkOption {
              type = types.str;
              description = "Listening address/port for http server.";
              default = "0.0.0.0:2525";
            };
          };
          config = lib.mkIf cfg.enable {
            systemd.services.mail-blackhole = {
              description = "Mail Blackhole service";
              wantedBy = ["multi-user.target"];
              after = ["network.target"];

              serviceConfig = {
                StateDirectory = "mail-blackhole/mails";
                ExecStart = "${cfg.package}/bin/mail-blackhole --listen-http ${cfg.listen-http} --listen-mail ${cfg.listen-mail} --mailboxes /var/lib/mail-blackhole/mails";
              };
            };
          };
        };
    };
}
