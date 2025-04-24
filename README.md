# Narukami Resource Generator

Resource generator for a certain browser tank game.
It generates resource files for game from a human-editable source files.

## Usage

Resources are read from the `resources` directory.
Generated files are written to the `out` directory.

```sh
RUST_LOG=info cargo run --release
```

### Podman (alternative)

I develop without Podman, so this isn't guaranteed to always work.

Build an OCI image first:

```sh
podman build -t narukami/resource-server:dev .
```

Now you can run the resource generator.
The first time the container is run it will clone resources into `/app/resources/`, the next time it will do `git pull --ff-only`.

```sh
mkdir -p data/resources # Input directory
mkdir -p data/out # Output directory

podman run --rm \
  -v $(realpath data/resources):/app/resources \
  -v $(realpath data/out):/app/out \
  narukami/resource-server:dev
```

## License

Licensed under GNU Affero General Public License, version 3.0 or later ([LICENSE](LICENSE) or https://www.gnu.org/licenses/agpl-3.0.html).
