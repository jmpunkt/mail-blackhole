{
  config,
  lib,
  pkgs,
  ...
}: {
  services.mail-blackhole = {
    enable = true;
    listen-mail = "0.0.0.0:2525";
  };

  users = {
    mutableUsers = false;
    users.root = {
      password = "";
      isSystemUser = true;
    };
  };

  virtualisation = {
    qemu = {
      package = pkgs.qemu;
      networkingOptions = [
        # We need to re-define our usermode network driver
        # since we are overriding the default value.
        "-net nic,netdev=user.1,model=virtio"
        # Than we can use qemu's hostfwd option to forward ports.
        "-netdev user,id=user.1,hostfwd=tcp::8000-:8000,hostfwd=tcp::22222-:22222"
      ];
    };
  };

  services.openssh = {
    ports = [22222];
    enable = true;
    settings = {
      PermitRootLogin = "yes";
    };
  };

  networking.firewall.allowedTCPPorts = [8000 22222];

  environment.systemPackages = [pkgs.msmtp];

  services.nginx = {
    enable = true;
    recommendedProxySettings = true;

    virtualHosts = {
      "127.0.0.1" = {
        listen = [
          {
            addr = "0.0.0.0";
            port = 8000;
          }
        ];

        locations."/mails/".extraConfig = ''
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
          proxy_set_header Host $host;
          proxy_redirect off;
          proxy_pass http://127.0.0.1:8080/mails/;
        '';
      };
    };
  };
}
