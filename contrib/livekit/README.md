# Your own video server

Discofast can send camera and screen between friends through a
[LiveKit](https://livekit.io) server you run, beside the Discord call. The
sound stays on Discord. Everyone in the same voice channel with the same
key sees each other.

Discofast must be built with the `selfhosted` feature:

```sh
cargo run --release --features selfhosted
```

## On the server

You need a machine with a public address, Docker and a domain name
pointing at it.

1. Copy this folder to the server.
2. Make a key and a secret:
   `docker run --rm livekit/livekit-server:v1.13.11 generate-keys`
3. Put them in `livekit.yaml` under `keys`, and your domain in `Caddyfile`.
4. Open these ports in the firewall: 80/tcp and 443/tcp (the certificate
   and the connection), 7881/tcp, and 50000-50200/udp (the video).
5. `docker compose up -d`

## In Discofast

User settings, Voz e vídeo, "Servidor de vídeo próprio":

- Endereço: `wss://` and your domain, for example `wss://video.example.com`
- API key and API secret: the ones from step 2. The secret is kept in the
  OS keyring.

Join a voice channel; the camera and screen buttons in the voice panel
light up once the video server is connected.

Whoever has the secret can do anything on the server, so give it only to
friends you trust.
