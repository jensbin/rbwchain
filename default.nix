{ lib
, rustPlatform
}:
let
  fs = lib.fileset;
  sourceFiles = fs.unions [
    ./Cargo.toml
    ./Cargo.lock
    ./src/main.rs
  ];
in
rustPlatform.buildRustPackage {
  pname = "rbwchain";
  version = "0.0.1";

  src = fs.toSource {
    root = ./.;
    fileset = sourceFiles;
  };

  # cargoHash = lib.fakeHash;
  cargoLock = {                                                                                                                                                     
    lockFile = ./Cargo.lock;                                                                                                                                        
  };                       

  meta = with lib; {
    description = "Executes a command with secrets from rbw";
    license = licenses.mit;
  };
}
