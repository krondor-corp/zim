use async_trait::async_trait;
use uuid::Uuid;

use zim_core::linked_data::Link;
use zim_protocol::log::BucketLogProvider;

use crate::database::models::{BucketLogEntry, BucketStatus};
use crate::database::{Database, DatabaseError};

#[async_trait]
impl BucketLogProvider for Database {
    type Error = DatabaseError;

    async fn exists(
        &self,
        id: Uuid,
    ) -> Result<bool, zim_protocol::log::BucketLogError<Self::Error>> {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            BucketLogEntry::bucket_exists(&id, &db)
                .map_err(zim_protocol::log::BucketLogError::Provider)
        })
        .await
        .map_err(|e| {
            zim_protocol::log::BucketLogError::Provider(DatabaseError::Deserialize(e.into()))
        })?
    }

    async fn heads(
        &self,
        id: Uuid,
        height: u64,
    ) -> Result<Vec<Link>, zim_protocol::log::BucketLogError<Self::Error>> {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            BucketLogEntry::heads_at(&id, height, &db)
                .map_err(zim_protocol::log::BucketLogError::Provider)
        })
        .await
        .map_err(|e| {
            zim_protocol::log::BucketLogError::Provider(DatabaseError::Deserialize(e.into()))
        })?
    }

    async fn append(
        &self,
        id: Uuid,
        name: String,
        current: Link,
        previous: Option<Link>,
        height: u64,
        published: bool,
    ) -> Result<(), zim_protocol::log::BucketLogError<Self::Error>> {
        if previous.is_none() && height != 0 {
            return Err(zim_protocol::log::BucketLogError::InvalidAppend(
                current,
                Link::default(),
                height,
            ));
        }

        if let Some(ref prev_link) = previous {
            if height == 0 {
                return Err(zim_protocol::log::BucketLogError::InvalidAppend(
                    current,
                    prev_link.clone(),
                    height,
                ));
            }
        }

        let db = self.clone();
        let prev_clone = previous.clone();
        let current_clone = current.clone();
        tokio::task::spawn_blocking(move || {
            if let Some(ref prev_link) = prev_clone {
                if !BucketLogEntry::exists_at(&id, prev_link, height - 1, &db)
                    .map_err(zim_protocol::log::BucketLogError::Provider)?
                {
                    return Err(zim_protocol::log::BucketLogError::InvalidAppend(
                        current_clone.clone(),
                        prev_link.clone(),
                        height,
                    ));
                }
            }

            BucketLogEntry::append(
                &id,
                &name,
                &current_clone,
                prev_clone.as_ref(),
                height,
                published,
                &db,
            )
            .map_err(|e| match e {
                DatabaseError::Client(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    zim_protocol::log::BucketLogError::Conflict
                }
                other => zim_protocol::log::BucketLogError::Provider(other),
            })
        })
        .await
        .map_err(|e| {
            zim_protocol::log::BucketLogError::Provider(DatabaseError::Deserialize(e.into()))
        })?
    }

    async fn height(
        &self,
        id: Uuid,
    ) -> Result<u64, zim_protocol::log::BucketLogError<Self::Error>> {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            BucketLogEntry::max_height(&id, &db)
                .map_err(zim_protocol::log::BucketLogError::Provider)?
                .ok_or(zim_protocol::log::BucketLogError::HeadNotFound(0))
        })
        .await
        .map_err(|e| {
            zim_protocol::log::BucketLogError::Provider(DatabaseError::Deserialize(e.into()))
        })?
    }

    async fn has(
        &self,
        id: Uuid,
        link: Link,
    ) -> Result<Vec<u64>, zim_protocol::log::BucketLogError<Self::Error>> {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            BucketLogEntry::heights_for_link(&id, &link, &db)
                .map_err(zim_protocol::log::BucketLogError::Provider)
        })
        .await
        .map_err(|e| {
            zim_protocol::log::BucketLogError::Provider(DatabaseError::Deserialize(e.into()))
        })?
    }

    async fn list_buckets(
        &self,
    ) -> Result<Vec<Uuid>, zim_protocol::log::BucketLogError<Self::Error>> {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            BucketLogEntry::list_bucket_ids(&db)
                .map_err(zim_protocol::log::BucketLogError::Provider)
        })
        .await
        .map_err(|e| {
            zim_protocol::log::BucketLogError::Provider(DatabaseError::Deserialize(e.into()))
        })?
    }

    async fn latest_published(
        &self,
        id: Uuid,
    ) -> Result<Option<(Link, u64)>, zim_protocol::log::BucketLogError<Self::Error>> {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            BucketLogEntry::latest_published(&id, &db)
                .map_err(zim_protocol::log::BucketLogError::Provider)
        })
        .await
        .map_err(|e| {
            zim_protocol::log::BucketLogError::Provider(DatabaseError::Deserialize(e.into()))
        })?
    }

    async fn should_sync_content(
        &self,
        id: Uuid,
    ) -> Result<bool, zim_protocol::log::BucketLogError<Self::Error>> {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            let status = BucketStatus::get_effective(&id, &db)
                .map_err(zim_protocol::log::BucketLogError::Provider)?;
            Ok(status == BucketStatus::Active)
        })
        .await
        .map_err(|e| {
            zim_protocol::log::BucketLogError::Provider(DatabaseError::Deserialize(e.into()))
        })?
    }

    async fn on_new_bucket_discovered(
        &self,
        id: Uuid,
        shared_by: Option<String>,
    ) -> Result<(), zim_protocol::log::BucketLogError<Self::Error>> {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            BucketStatus::set(&id, BucketStatus::Pending, shared_by.as_deref(), &db)
                .map_err(zim_protocol::log::BucketLogError::Provider)
        })
        .await
        .map_err(|e| {
            zim_protocol::log::BucketLogError::Provider(DatabaseError::Deserialize(e.into()))
        })?
    }

    async fn list_syncable_buckets(
        &self,
    ) -> Result<Vec<Uuid>, zim_protocol::log::BucketLogError<Self::Error>> {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            BucketLogEntry::list_syncable_bucket_ids(&db)
                .map_err(zim_protocol::log::BucketLogError::Provider)
        })
        .await
        .map_err(|e| {
            zim_protocol::log::BucketLogError::Provider(DatabaseError::Deserialize(e.into()))
        })?
    }
}
