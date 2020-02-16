let
  pkgs = import <nixpkgs> {};
in
pkgs.mkShell {
  buildInputs = with pkgs; [
    protobuf
  ];
  PROTOC = "${pkgs.protobuf}/bin/protoc";
}
