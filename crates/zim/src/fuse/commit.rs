//! Debounced commits for a mounted vault.
//!
//! An editor's save is a burst of syscalls, not one: vim's atomic save
//! probes writability (`4913`), renames `note.md → note.md~`, creates an
//! empty `note.md`, writes, flushes, then deletes the backup. Committing
//! (and announcing) per syscall turns that burst into a string of real,
//! synced versions — the empty `create` among them — that race to remote
//! peers ahead of the final content.
//!
//! [`PendingCommit`] separates *staging* from *committing*: FUSE mutations
//! apply to the in-memory tree (so reads through the mount see them at
//! once) and [`PendingCommit::touch`] the pending state; one `save()` +
//! announce happens after the mount has been quiet for [`QUIET_WINDOW`].
//! [`MAX_DELAY`] caps how long a never-quiet writer can defer a commit.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::RwLock;
use tokio::time::Instant;

use zim_core::blobs::BlobStore;
use zim_core::vault::log::VaultLog;
use zim_core::vault::{Vault, VaultError};

/// Quiet period after the last staged mutation before it commits. Long
/// enough to swallow an editor's save burst (milliseconds end to end).
pub const QUIET_WINDOW: Duration = Duration::from_millis(500);

/// Longest a staged mutation waits for a quiet period before committing
/// anyway — a steady writer (a log appender) still gets versions.
pub const MAX_DELAY: Duration = Duration::from_secs(5);

/// Fired after each commit (the daemon wires this to `Peer::announce_head`).
/// Called from inside the runtime, so it must not block on it — spawn.
pub type OnCommit = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct State {
    /// Bumped by every [`PendingCommit::touch`]; a debounce timer only
    /// fires if no later touch superseded it.
    generation: u64,
    /// When the oldest uncommitted mutation was staged; `None` = clean.
    dirty_since: Option<Instant>,
}

/// Staged-but-uncommitted mutations on a mounted vault, and the timer that
/// commits them. Shared by the FUSE filesystem (which stages) and the mount
/// manager (which must commit staged work before reloading or routing an
/// HTTP write through the same handle).
pub struct PendingCommit<B: BlobStore, L: VaultLog> {
    rt: Handle,
    vault: Arc<RwLock<Vault<B, L>>>,
    state: Mutex<State>,
    on_commit: Option<OnCommit>,
    quiet: Duration,
    max_delay: Duration,
}

impl<B: BlobStore, L: VaultLog> PendingCommit<B, L> {
    pub fn new(rt: Handle, vault: Arc<RwLock<Vault<B, L>>>, on_commit: Option<OnCommit>) -> Self {
        Self::with_timing(rt, vault, on_commit, QUIET_WINDOW, MAX_DELAY)
    }

    pub fn with_timing(
        rt: Handle,
        vault: Arc<RwLock<Vault<B, L>>>,
        on_commit: Option<OnCommit>,
        quiet: Duration,
        max_delay: Duration,
    ) -> Self {
        Self {
            rt,
            vault,
            state: Mutex::new(State::default()),
            on_commit,
            quiet,
            max_delay,
        }
    }

    /// Record a staged mutation and (re)arm the commit timer. Call while
    /// still holding the vault's write guard, so a concurrent commit can't
    /// slip between the stage and the mark.
    pub fn touch(self: &Arc<Self>) {
        let generation = {
            let mut s = self.state.lock().unwrap();
            s.generation += 1;
            s.dirty_since.get_or_insert_with(Instant::now);
            s.generation
        };
        let this = self.clone();
        self.rt.spawn(async move {
            tokio::time::sleep(this.quiet).await;
            if !this.due(generation) {
                return;
            }
            if let Err(e) = this.flush().await {
                tracing::warn!("mount commit failed: {e}");
            }
        });
    }

    /// Whether the timer armed at `generation` should commit: nothing was
    /// staged since, or the oldest staged mutation has waited long enough.
    fn due(&self, generation: u64) -> bool {
        let s = self.state.lock().unwrap();
        match s.dirty_since {
            None => false,
            Some(since) => s.generation == generation || since.elapsed() >= self.max_delay,
        }
    }

    /// Commit any staged mutations now. Returns whether a version was saved.
    pub async fn flush(&self) -> Result<bool, VaultError<L::Error>> {
        let mut guard = self.vault.write().await;
        self.flush_locked(&mut guard).await
    }

    /// [`Self::flush`] for a caller already holding the write guard.
    pub async fn flush_locked(
        &self,
        vault: &mut Vault<B, L>,
    ) -> Result<bool, VaultError<L::Error>> {
        let Some(since) = self.state.lock().unwrap().dirty_since.take() else {
            return Ok(false);
        };
        if let Err(e) = vault.save().await {
            // Keep it pending: the next touch or flush retries.
            let mut s = self.state.lock().unwrap();
            s.dirty_since = Some(s.dirty_since.map_or(since, |d| d.min(since)));
            return Err(e);
        }
        if let Some(cb) = &self.on_commit {
            cb();
        }
        Ok(true)
    }

    /// Mark staged work as committed by someone else holding the write
    /// guard (a routed HTTP mutation whose `save()` swept it up).
    pub fn mark_committed(&self) {
        self.state.lock().unwrap().dirty_since = None;
    }

    #[cfg(test)]
    fn is_dirty(&self) -> bool {
        self.state.lock().unwrap().dirty_since.is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use zim_core::fs::AbsPath;
    use zim_crypto::PrivateKey;
    use zim_peer::{BlobsProvider, MemoryVaultLog};

    use super::*;

    type TestVault = Vault<BlobsProvider, MemoryVaultLog>;

    const QUIET: Duration = Duration::from_millis(50);

    fn abs(p: &str) -> AbsPath {
        AbsPath::from_abs(std::path::PathBuf::from(p))
    }

    /// Alice's freshly-initialised vault, plus a counter of announces.
    async fn alice_mount(
        max_delay: Duration,
    ) -> (
        Arc<RwLock<TestVault>>,
        Arc<PendingCommit<BlobsProvider, MemoryVaultLog>>,
        Arc<AtomicUsize>,
    ) {
        let blobs = BlobsProvider::memory().await.unwrap();
        let vault = Vault::init(
            "notes".into(),
            &PrivateKey::generate(),
            blobs,
            MemoryVaultLog::new(),
        )
        .await
        .unwrap();
        let vault = Arc::new(RwLock::new(vault));
        let announces = Arc::new(AtomicUsize::new(0));
        let counter = announces.clone();
        let on_commit: OnCommit = Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        let pending = Arc::new(PendingCommit::with_timing(
            Handle::current(),
            vault.clone(),
            Some(on_commit),
            QUIET,
            max_delay,
        ));
        (vault, pending, announces)
    }

    /// Stage `data` at `path` the way the FUSE layer does: under the write
    /// guard, touching the pending commit before releasing it.
    async fn stage_write(
        vault: &Arc<RwLock<TestVault>>,
        pending: &Arc<PendingCommit<BlobsProvider, MemoryVaultLog>>,
        path: &str,
        data: &[u8],
    ) {
        let g = vault.write().await;
        g.fs()
            .add(&abs(path), Cursor::new(data.to_vec()))
            .await
            .unwrap();
        pending.touch();
    }

    async fn stage_mv(
        vault: &Arc<RwLock<TestVault>>,
        pending: &Arc<PendingCommit<BlobsProvider, MemoryVaultLog>>,
        from: &str,
        to: &str,
    ) {
        let g = vault.write().await;
        g.fs().mv(&abs(from), &abs(to)).await.unwrap();
        pending.touch();
    }

    async fn stage_rm(
        vault: &Arc<RwLock<TestVault>>,
        pending: &Arc<PendingCommit<BlobsProvider, MemoryVaultLog>>,
        path: &str,
    ) {
        let g = vault.write().await;
        g.fs().rm(&abs(path)).await.unwrap();
        pending.touch();
    }

    #[tokio::test(start_paused = true)]
    async fn vim_atomic_save_lands_as_one_commit_with_the_full_content() {
        let (vault, pending, announces) = alice_mount(MAX_DELAY).await;

        // Alice already has a note, committed.
        stage_write(&vault, &pending, "/note.md", b"first draft").await;
        tokio::time::sleep(QUIET * 2).await;
        let height_before = vault.read().await.height();
        assert_eq!(announces.load(Ordering::SeqCst), 1);

        // She `:w`s in vim: probe file, backup rename, empty create,
        // full write on flush, backup + probe cleanup.
        stage_write(&vault, &pending, "/4913", b"").await;
        stage_rm(&vault, &pending, "/4913").await;
        stage_mv(&vault, &pending, "/note.md", "/note.md~").await;
        stage_write(&vault, &pending, "/note.md", b"").await;
        stage_write(&vault, &pending, "/note.md", b"first draft, edited in vim").await;
        stage_rm(&vault, &pending, "/note.md~").await;

        // Mid-burst nothing is committed or announced…
        assert_eq!(vault.read().await.height(), height_before);
        assert_eq!(announces.load(Ordering::SeqCst), 1);

        // …once the mount goes quiet, the whole save is a single version.
        tokio::time::sleep(QUIET * 2).await;
        let g = vault.read().await;
        assert_eq!(g.height(), height_before + 1);
        assert_eq!(announces.load(Ordering::SeqCst), 2);
        assert_eq!(
            g.fs().cat(&abs("/note.md")).await.unwrap(),
            b"first draft, edited in vim"
        );
        assert!(g.fs().cat(&abs("/note.md~")).await.is_err());
        assert!(g.fs().cat(&abs("/4913")).await.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn a_writer_that_never_pauses_still_commits_after_the_max_delay() {
        let max_delay = QUIET * 4;
        let (vault, pending, _) = alice_mount(max_delay).await;

        // Alice's logger appends faster than the quiet window, for longer
        // than the max delay.
        let mut log = Vec::new();
        for i in 0..10 {
            log.extend_from_slice(format!("line {i}\n").as_bytes());
            stage_write(&vault, &pending, "/app.log", &log).await;
            tokio::time::sleep(QUIET / 2).await;
        }

        assert!(
            vault.read().await.height() > 0,
            "a steady writer must not defer commits forever"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn flush_commits_staged_work_immediately_and_only_once() {
        let (vault, pending, announces) = alice_mount(MAX_DELAY).await;

        stage_write(&vault, &pending, "/todo.md", b"buy milk").await;
        assert!(pending.flush().await.unwrap(), "staged work commits");
        assert!(!pending.is_dirty());
        let height = vault.read().await.height();

        // The armed timer finds nothing left to do, and a second flush is
        // a no-op — no empty versions.
        tokio::time::sleep(QUIET * 2).await;
        assert!(!pending.flush().await.unwrap());
        assert_eq!(vault.read().await.height(), height);
        assert_eq!(announces.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_routed_save_that_swept_up_staged_work_cancels_the_timer() {
        let (vault, pending, announces) = alice_mount(MAX_DELAY).await;

        // Alice writes through the mount, then Bob's HTTP-API write to the
        // same vault saves under the guard before the timer fires.
        stage_write(&vault, &pending, "/a.md", b"from the mount").await;
        {
            let mut g = vault.write().await;
            g.fs()
                .add(&abs("/b.md"), Cursor::new(b"from the api".to_vec()))
                .await
                .unwrap();
            g.save().await.unwrap();
            pending.mark_committed();
        }
        let height = vault.read().await.height();

        tokio::time::sleep(QUIET * 2).await;
        assert_eq!(vault.read().await.height(), height, "no empty follow-up");
        assert_eq!(announces.load(Ordering::SeqCst), 0);
    }
}
