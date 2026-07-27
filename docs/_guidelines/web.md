# Curating the End-User Site

The [`web/`](../../web/) directory is Zim's public site and end-user
documentation. It is for people evaluating, installing, and using Zim, not for
contributors navigating the implementation.

## Rules

- **Use plain, task-oriented language.** Lead with what the reader is trying to
  accomplish.
- **Include only necessary internals.** Commands, configuration keys, URLs, and
  security implications are appropriate when users need them. Crate paths,
  Rust types, database schemas, and implementation tours are not.
- **Make instructions executable.** Commands should be copy-pasteable and say
  where they run and what success looks like.
- **Describe shipped behavior only.** Do not present planned features as
  available.
- **Keep security boundaries explicit.** State which machine sees plaintext,
  which credentials are required, and what trust is placed in peers or hubs.
- **Update changed workflows.** CLI, configuration, authentication, and install
  changes are incomplete until their user-facing pages match.

Contributor and agent documentation belongs in [`docs/`](../index.md).

## Site Structure

| Path | Purpose |
|---|---|
| `web/_docs/*.md` | End-user documentation pages |
| `web/_data/nav.yml` | Documentation sidebar and page order |
| `web/_layouts/` | Jekyll page layouts |
| `web/index.md` | Site home page content |
| `web/assets/` | Site styles and images |

Run the site locally with `make -C web dev`. Build it once with
`make -C web build`; Jekyll writes generated output to the gitignored
`web/_site/` directory.
