# Draft: Mirror section for `wiki/_docs/local-development.md`

**Audience:** wiki (end-user). Operational, copy-pasteable. No Rust internals.
**For:** thing4 to splice into `wiki/_docs/local-development.md`.
**Owner:** thing3 (T-016d). Applies on top of thing4's existing local-dev page.

Suggested placement: after the existing "Run zim-hub (`make hub`)" section, before any "next steps" footer.

---

## Mirror a bucket on zim-hub

zim-hub acts as a **mirror peer** — it holds and serves a bucket's public files without ever holding the bucket's secret. To put a bucket onto a hub, the bucket's owner pre-authorizes the hub's peer key.

### One-time setup per bucket

1. **Start zim-hub.** On first boot it prints its node id and a ready-to-copy command:

   ```
   To mirror a bucket on this hub, run on the owning peer:
     zim bucket mirror add <BUCKET_ID> 1ea75079a6bc194f4b3e28dad40b49c8762ae0832fcba25ff043c1ff7f7ced81
   ```

2. **On the owning peer** (the machine running `zim` as a member of the bucket), substitute the bucket id and run the command:

   ```
   zim bucket mirror add <YOUR_BUCKET_ID> <HUB_NODE_ID>
   ```

3. The hub fetches the bucket's public files and surfaces them at `http://localhost:8080/b/<BUCKET_ID>/tree`.

### Stable node id

The hub's node id stays the same across restarts as long as you keep the `data` directory (default `./data/zim-hub/`). The directory holds the iroh secret key.

- **Move the data directory** → the same node id moves with it. Mirroring keeps working.
- **Delete the data directory** → new node id on next boot. You'll need to re-authorize with `zim bucket mirror add` against the new id.

### What gets mirrored

Only files explicitly marked public in the bucket. zim-hub never sees the bucket secret, so private content is never decrypted on the hub side.

### Multi-bucket

Run `zim bucket mirror add` once per bucket you want the hub to serve. The hub aggregates them — `http://localhost:8080/` lists every mirrored bucket.

### Removing a mirror

On the owning peer:

```
zim bucket mirror remove <BUCKET_ID> <HUB_NODE_ID>
```

The hub keeps the blobs it already fetched (no auto-eviction in v1) but stops receiving new ones. To free disk space, delete the bucket's contents from the hub's data directory manually.

---

## Notes for thing4

- Commands shown (`zim bucket mirror add` / `remove`) **don't exist yet** as of T-016d. T-016b (thing1) adds them. This wiki section can land now and the commands will work once T-016b ships; if you'd rather hold the wiki edit, flag it in T-016d's Notes and I'll redraft to mark them "coming in T-016b".
- The "data directory" guidance (`./data/zim-hub/`) tracks the `ZIM_HUB_DATA` env var documented in the `.env.example` and `crates/zim-hub/README.md`.
- The wiki page audience is end-user, so I've avoided the protocol-level terms ("mirror peer type", "iroh", "secret key" beyond "iroh secret key" once). Trim further if your house style prefers.
