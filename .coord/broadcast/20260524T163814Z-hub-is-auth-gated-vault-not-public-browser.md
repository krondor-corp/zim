---
from: orch
ts: 20260524T163814Z
kind: policy
audience: all
---
# CORE TENET: zim-hub is an auth-gated identity vault. Unauthenticated visitors see marketing, nothing else.

Product direction (binding, corrects a coordination miss between T-002/T-003 and T-001):

zim-hub is **primarily an authenticated identity key vault** (per T-001). Bucket browsing, published-content viewing, and any peer/mirror functionality lives **behind authentication**. An unauthenticated visitor hitting `http://localhost:8080/` must see a **marketing landing page** — nothing else.

## The miss

T-002/T-003 framed zim-hub as a "GitHub-like read-only mirror gateway" with public bucket-browsing routes (`/b/{id}/tree/*`, `/b/{id}/blob/*`, etc.). thing3 built M3 against that framing. Then T-001 redefined the hub as an identity vault. I never told thing3 to gate the M3 surface or replace `/` with a marketing page. Result: the working hub shows bucket-browsing UI to anyone who hits port 8080. That's wrong.

## Target shape

- **`GET /`** unauthenticated → marketing landing page ("Welcome to Zim. Sign in to access your buckets."). One page, no bucket data, no peer state. Clean-break: no "empty state" peek at the system.
- **`GET /`** authenticated → dashboard (whatever that ends up being — could be "your buckets" or "your published-set" or just an account home).
- **All `/b/{id}/*` routes** require auth. Unauthenticated → 302 to `/login`.
- **`GET /login`** unauthenticated → Google OAuth flow (or "OAuth not configured" status page in dev mode).
- **All `/api/v0/*` routes** that touch peer/bucket data require auth.
- **Mirror serving** (T-016 published-set surface, when T-016a/b/c/T-008a/b land) is a separate question — that's the hub-as-mirror behavior serving published content over its peer. If/when the hub serves published files to anonymous consumers (per T-016 Decision 3, anon gets manifest only), that's a different route surface from `/b/*`.

## What to delete or gate

- **Public `/` bucket-listing page** — replace with marketing page (auth dependent).
- **`/b/{id}/tree/*`, `/b/{id}/blob/*`, `/b/{id}/raw/*`, `/b/{id}/history`** — gate behind session middleware.
- **Anything else from T-002 M3** that leaks bucket state without auth.

## Sequencing

thing3 is mid-T-001a M3 (enrolment flow). This correction folds in:
- **M3a (new)**: replace `/` with marketing template; add session-required middleware over `/b/*` and `/api/v0/buckets/*`.
- **M3b**: rest of enrolment flow as planned.

## Why this matters

The hub's threat model (T-001) assumes the hub is auth-gated. An anonymous bucket-browsing surface leaks data the threat model promised wouldn't leak. Has to go.
