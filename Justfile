run:
  cargo run

updeps:
  cargo upgrade --verbose
  cargo update --verbose
  cargo outdated

test:
  cargo nextest run

coverage:
  cargo llvm-cov nextest --text

podman-build:
  podman build . -t docker.io/rpodgorny/faddnsd

podman-push: podman-build
  podman push docker.io/rpodgorny/faddnsd
