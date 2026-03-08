{
  description = "A Nix plugin that adds builtins.helloWorld using nix-bindings";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.05";
    flake-utils.url = "github:numtide/flake-utils";
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, naersk }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # Nix 2.32 is required to match the nix-bindings-sys = "2.32.4" crate version.
        # The C API headers and libraries must match the crate's expected version.
        nixForPlugin = pkgs.nixVersions.nix_2_32;

        naersk' = pkgs.callPackage naersk { };

        # Build the plugin as a cdylib (shared library)
        plugin = naersk'.buildPackage {
          pname = "nix-hello-world-plugin";
          version = "0.1.0";
          src = ./plugin;

          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.rustPlatform.bindgenHook  # provides libclang for bindgen
          ];

          buildInputs = [
            nixForPlugin.dev   # Nix C API headers (nix_api_value.h, etc.)
            nixForPlugin       # Nix libraries for linking
          ];

          # Ensure the .so ends up in the lib/ output directory
          postInstall = ''
            mkdir -p $out/lib
            find target -name "libnix_hello_world_plugin.so" -exec cp {} $out/lib/ \;
          '';
        };

      in
      {
        packages = {
          default = plugin;
          nix-hello-world-plugin = plugin;
        };

        # Development shell with all build deps available
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [
            pkgs.cargo
            pkgs.rustc
            pkgs.rustfmt
            pkgs.clippy
            pkgs.pkg-config
            pkgs.rustPlatform.bindgenHook
          ];
          buildInputs = [
            nixForPlugin.dev
            nixForPlugin
          ];

          # Point pkg-config at the Nix dev libraries
          shellHook = ''
            echo "Nix plugin dev shell. Build with: cargo build --release"
            echo "Test with:"
            echo "  nix eval --plugin-files ./target/release/libnix_hello_world_plugin.so \\"
            echo "           --expr 'builtins.helloWorld null'"
          '';
        };

        # Quick check: build and run the eval to verify the builtin works
        checks.hello-world-works = pkgs.runCommand "check-hello-world-plugin" {
          buildInputs = [ nixForPlugin ];
          pluginLib = "${plugin}/lib/libnix_hello_world_plugin.so";
        } ''
          result=$(nix eval --plugin-files "$pluginLib" --expr 'builtins.helloWorld null' 2>&1)
          if [ "$result" = '"Hello, World!"' ]; then
            echo "PASS: builtins.helloWorld null = $result"
            touch $out
          else
            echo "FAIL: got '$result', expected '\"Hello, World!\"'"
            exit 1
          fi
        '';
      }
    );
}
