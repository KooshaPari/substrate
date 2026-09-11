{
  description = "phenotype-router — dev shell (ADR-039: pheno-flake refresh template)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in {
        devShells.default = pkgs.mkShell {
          name = "phenotype-router";
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            cargo-llvm-cov
            rust-analyzer
          ];
          shellHook = ''
            echo "phenotype-router dev shell (ADR-039 / pheno-flake refresh template)"
            echo ""
            echo "Quick start:"
            echo "  cargo test                  # unit + integ + e2e (28 tests)"
            echo "  cargo test --features chaos # + chaos matrix (56 tests)"
            echo "  cargo bench --bench decision # criterion (3 benchmarks)"
            echo "  cargo llvm-cov --lcov --output-path lcov.info  # coverage gate"
            echo ""
          '';
        };
      });
}
