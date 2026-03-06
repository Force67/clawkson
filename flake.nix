{
  description = "Clawkson – multi-agent AI assistant platform";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rust = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "rust-analyzer" ];
          };
        in
        {
          default = pkgs.mkShell {
            buildInputs = [
              rust
              pkgs.pkg-config
              pkgs.openssl

              # Database
              pkgs.postgresql
              pkgs.sqlx-cli
              pkgs.docker-compose
            ];

            env = {
              RUST_BACKTRACE = "1";
              # reqwest uses rustls, but openssl is needed for sqlx-cli / pg native
              OPENSSL_DIR = "${pkgs.openssl.dev}";
              OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
            };

            shellHook = ''
              echo "clawkson dev shell"
              echo "  rust   : $(rustc --version)"
              echo "  cargo  : $(cargo --version)"
              echo "  psql   : $(psql --version)"
            '';
          };
        });
    };
}
