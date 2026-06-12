# Alice / Bob p2p sync regression test

Two daemons, one shared vault. Verifies the core peer-to-peer sync
path: alice writes → bob auto-bootstraps → bob writes → alice
auto-pulls. No hub involved.

Catches the regressions that landed earlier:
- `Vault::save()` not advancing the local log (heads stuck at 1)
- `Vault::init()` not snapshotting the metadata pack into the
  genesis manifest (vault can't be re-opened after creation)
- `open_vault` cache returning stale manifest clones across handlers

## Steps

```bash
# 1. Clean slate
./bin/dev kill --force
./bin/dev clean

# 2. Boot both daemons in background tmux
./bin/dev run --background
sleep 4

# 3. Initialize each peer's state
./bin/dev cli alice init
./bin/dev cli bob   init

# 4. Tell each daemon about the other (exchange did:keys)
ALICE_PK=$(./bin/dev cli alice id | tail -1 | tr -d '\r\n')
BOB_PK=$(  ./bin/dev cli bob   id | tail -1 | tr -d '\r\n')
./bin/dev cli alice peers add bob   "$BOB_PK"
./bin/dev cli bob   peers add alice "$ALICE_PK"

# 5. Alice creates a vault + writes a file
./bin/dev cli alice vaults create demo
echo "from alice" | ./bin/dev cli alice vault demo add /readme.md

# 6. Alice shares with bob — the OfferShare effect bootstraps bob
./bin/dev cli alice vault demo shares add bob
sleep 12   # DHT discovery + OfferShare → bob's vault open

# 7. Bob can see + read alice's file
./bin/dev cli bob vaults list
./bin/dev cli bob vault demo ls /
./bin/dev cli bob vault demo cat /readme.md
# expect: "from alice"

# 8. Bob writes — announce_head fires → alice pulls
echo "from bob" | ./bin/dev cli bob vault demo add /from_bob.txt
sleep 5

# 9. Alice has bob's file
./bin/dev cli alice vault demo ls /
./bin/dev cli alice vault demo cat /from_bob.txt
# expect: "from bob"

# 10. Both heights advanced past 1 (the cache-bug regression)
./bin/dev cli alice vault demo head
./bin/dev cli bob   vault demo head

# 11. Cleanup
./bin/dev kill --force
```

## Expected outcome

| Check | Pass condition |
|---|---|
| `bob vaults list` after share | shows `demo` |
| `bob vault demo cat /readme.md` | prints `from alice` |
| `alice vault demo cat /from_bob.txt` | prints `from bob` |
| Both `head` heights | `≥ 3` (genesis + add + share + bob's write, all in chain) |

If either `cat` errors with `path not found`, OR either `head`
reports height `≤ 1`, the sync regression is back — start with
`./bin/dev logs alice` and `./bin/dev logs bob`.

## Why 12 seconds before bob can read

The flow is:
1. Alice's `shares add bob` submits an `Effect::OfferShare` to the
   coordinator's effect queue.
2. The background runner dials bob via the pkarr DHT.
3. First-time DHT lookup takes 5–10s; subsequent dials are cached.
4. Once dialed, bob receives the `ShareOffered` wire message and
   bootstraps the vault into his local log + blob store.

The 5s wait in step 8 is shorter because the bob → alice direction
reuses the connection learned in step 6.
