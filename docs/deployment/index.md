# Deployment

## Release Process

See [release.md](./release.md) for the full release workflow (cargo-smart-release, GitHub Actions, crates.io publishing).

## Infrastructure (Future)

Kamal-based deployment and `iac/` infrastructure-as-code are planned but not yet scaffolded. When ready, configuration will live at:

- `iac/` — Terraform/Pulumi definitions
- `.kamal/` — Kamal deploy configs
- `config/` — Environment-specific settings
