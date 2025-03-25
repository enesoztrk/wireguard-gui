{
  wrapGAppsHook,
  fetchFromGitHub,
  rustPlatform,
  pkg-config,
  wireguard-tools,
  glib,
  gtk4,
  polkit,
}:
rustPlatform.buildRustPackage rec {
  pname = "wireguard-gui";
  version = "0.1.0";

  src = ./.;

  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook
  ];

  buildInputs = [
    wireguard-tools
    glib.dev
    gtk4.dev
    polkit
  ];

  postFixup = ''
    wrapProgram $out/bin/${pname} \
       --set LIBGL_ALWAYS_SOFTWARE true \
       --set G_MESSAGES_DEBUG all
  '';

  # cargoHash = "sha256-XO/saJfdiawN8CF6oF5HqrvLBllNueFUiE+7A7XWC5M=";
  cargoHash = "sha256-XO/saJfdiawN8CF6oF5HqrvLBllNueFUiE+7A7XWC5M=";
  # cargoHash = "";
}
