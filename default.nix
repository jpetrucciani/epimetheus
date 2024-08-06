{ pkgs ? import
    (fetchTarball {
      name = "jpetrucciani-2024-08-04";
      url = "https://github.com/jpetrucciani/nix/archive/5d293e2449312b4fef796dcd0836bf6bc1fad684.tar.gz";
      sha256 = "0z8dyf6d9mdka8gq3zchyy5ywzz91bj18xzdjnm0gpwify0pk312";
    })
    { }
}:
let
  name = "epimetheus";

  tools = with pkgs; {
    cli = [
      coreutils
      nixpkgs-fmt
    ];
    rust = [
      cargo
      clang
      rust-analyzer
      rustc
      rustfmt
      # deps
      pkg-config
      openssl
    ];
    scripts = pkgs.lib.attrsets.attrValues scripts;
  };

  scripts = with pkgs; let
    repo = "$(${pkgs.git}/bin/git rev-parse --show-toplevel)";
  in
  {
    test_server = pog {
      name = "test_server";
      script = ''
        ${srv}/bin/srv --directory "${repo}/test"
      '';
    };
  };
  paths = pkgs.lib.flatten [ (builtins.attrValues tools) ];
  env = pkgs.buildEnv {
    inherit name paths; buildInputs = paths;
  };
in
(env.overrideAttrs (_: {
  inherit name;
  NIXUP = "0.0.7";
})) // {
  inherit scripts;
}
