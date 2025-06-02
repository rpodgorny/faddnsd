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

tailwind:
  npx tailwindcss@3.4.17 -i ./static/input.css -o ./static/output.css --watch

vendor:
  wget https://github.com/bigskysoftware/htmx/releases/download/v2.0.4/htmx.min.js -O static/vendor/htmx.min.js
  git clone --depth=1 https://github.com/tkbviet/font-awesome-6.6.0-pro-full x && mkdir -p static/vendor/fontawesome && cp -r x/webfonts x/css ./static/vendor/fontawesome/ && rm -rf x

podman-build:
  podman build . -t docker.io/rpodgorny/faddnsd

podman-push: podman-build
  podman push docker.io/rpodgorny/faddnsd
