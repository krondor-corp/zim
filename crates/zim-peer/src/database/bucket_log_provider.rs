use async_trait::async_trait;
use uuid::Uuid;

use zim_protocol::log::BucketLogProvider;
use zim_store::linked_data::Link;

use crate::database::types::BucketStatus;
use crate::database::{types::DCid, Database};

#[async_trait]
impl BucketLogProvider for Database {
    type Error = sqlx::Error;

    async fn exists(
        &self,
        id: Uuid,
    ) -> Result<bool, zim_protocol::log::BucketLogError<Self::Error>> {
        let id_str = id.to_string();
        let result = sqlx::query!(
            r#"
            SELECT COUNT(*) as "count!: i64"
            FROM bucket_log
            WHERE bucket_id = $1
            "#,
            id_str
        )
        .fetch_one(&**self)
        .await
        .map_err(zim_protocol::log::BucketLogError::Provider)?;

        Ok(result.count > 0)
    }

    async fn heads(
        &self,
        id: Uuid,
        height: u64,
    ) -> Result<Vec<Link>, zim_protocol::log::BucketLogError<Self::Error>> {
        let height_i64 = height as i64;
        let id_str = id.to_string();

        let rows = sqlx::query!(
            r#"
            SELECT current_link as "current_link!: DCid"
            FROM bucket_log
            WHERE bucket_id = $1 AND height = $2
            "#,
            id_str,
            height_i64
        )
        .fetch_all(&**self)
        .await
        .map_err(zim_protocol::log::BucketLogError::Provider)?;

        Ok(rows.into_iter().map(|r| r.current_link.into()).collect())
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
        let current_dcid: DCid = current.clone().into();
        let previous_dcid: Option<DCid> = previous.clone().map(Into::into);
        let height_i64 = height as i64;

        // Validate: For genesis (previous_link is None), height should be 0
        if previous.is_none() && height != 0 {
            return Err(zim_protocol::log::BucketLogError::InvalidAppend(
                current,
                Link::default(),
                height,
            ));
        }

        // For non-genesis, validate that previous link exists at height - 1
        if let Some(prev_link) = previous.clone() {
            if height == 0 {
                return Err(zim_protocol::log::BucketLogError::InvalidAppend(
                    current, prev_link, height,
                ));
            }

            let prev_dcid: DCid = prev_link.clone().into();
            let prev_height = (height - 1) as i64;
            let id_str = id.to_string();

            let exists = sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM bucket_log
                WHERE bucket_id = $1 AND current_link = $2 AND height = $3
                "#,
                id_str,
                prev_dcid,
                prev_height
            )
            .fetch_one(&**self)
            .await
            .map_err(zim_protocol::log::BucketLogError::Provider)?;

            if exists.count == 0 {
                return Err(zim_protocol::log::BucketLogError::InvalidAppend(
                    current, prev_link, height,
                ));
            }
        }

        // Insert the log entry with name
        let id_str = id.to_string();
        sqlx::query!(
            r#"
            INSERT INTO bucket_log (bucket_id, name, current_link, previous_link, height, published, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, CURRENT_TIMESTAMP)
            "#,
            id_str,
            name,
            current_dcid,
            previous_dcid,
            height_i64,
            published
        )
        .execute(&**self)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db_error) => {
                if db_error.constraint().is_some() {
                    zim_protocol::log::BucketLogError::Conflict
                } else {
                    zim_protocol::log::BucketLogError::Provider(e)
                }
            }
            _ => zim_protocol::log::BucketLogError::Provider(e),
        })?;

        Ok(())
    }

    async fn height(
        &self,
        id: Uuid,
    ) -> Result<u64, zim_protocol::log::BucketLogError<Self::Error>> {
        let id_str = id.to_string();
        let result = sqlx::query!(
            r#"
            SELECT MAX(height) as "max_height: i64"
            FROM bucket_log
            WHERE bucket_id = $1
            "#,
            id_str
        )
        .fetch_one(&**self)
        .await
        .map_err(zim_protocol::log::BucketLogError::Provider)?;

        match result.max_height {
            Some(h) => Ok(h as u64),
            None => Err(zim_protocol::log::BucketLogError::HeadNotFound(0)),
        }
    }

    async fn has(
        &self,
        id: Uuid,
        link: Link,
    ) -> Result<Vec<u64>, zim_protocol::log::BucketLogError<Self::Error>> {
        let dcid: DCid = link.into();
        let id_str = id.to_string();

        let rows = sqlx::query!(
            r#"
            SELECT height
            FROM bucket_log
            WHERE bucket_id = $1 AND current_link = $2
            "#,
            id_str,
            dcid
        )
        .fetch_all(&**self)
        .await
        .map_err(zim_protocol::log::BucketLogError::Provider)?;

        Ok(rows.into_iter().map(|r| r.height as u64).collect())
    }

    async fn list_buckets(
        &self,
    ) -> Result<Vec<Uuid>, zim_protocol::log::BucketLogError<Self::Error>> {
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT bucket_id
            FROM bucket_log
            ORDER BY bucket_id
            "#
        )
        .fetch_all(&**self)
        .await
        .map_err(zim_protocol::log::BucketLogError::Provider)?;

        Ok(rows
            .into_iter()
            .map(|r| Uuid::parse_str(&r.bucket_id).expect("invalid bucket_id UUID in database"))
            .collect())
    }

    async fn latest_published(
        &self,
        id: Uuid,
    ) -> Result<Option<(Link, u64)>, zim_protocol::log::BucketLogError<Self::Error>> {
        let id_str = id.to_string();

        let result = sqlx::query!(
            r#"
            SELECT current_link as "current_link!: DCid", height
            FROM bucket_log
            WHERE bucket_id = $1 AND published = TRUE
            ORDER BY height DESC
            LIMIT 1
            "#,
            id_str
        )
        .fetch_optional(&**self)
        .await
        .map_err(zim_protocol::log::BucketLogError::Provider)?;

        Ok(result.map(|r| (r.current_link.into(), r.height as u64)))
    }

    async fn should_sync_content(
        &self,
        id: Uuid,
    ) -> Result<bool, zim_protocol::log::BucketLogError<Self::Error>> {
        let status = self
            .get_effective_bucket_status(&id)
            .await
            .map_err(zim_protocol::log::BucketLogError::Provider)?;
        Ok(status == BucketStatus::Active)
    }

    async fn on_new_bucket_discovered(
        &self,
        id: Uuid,
        shared_by: Option<String>,
    ) -> Result<(), zim_protocol::log::BucketLogError<Self::Error>> {
        self.set_bucket_status(&id, BucketStatus::Pending, shared_by.as_deref())
            .await
            .map_err(zim_protocol::log::BucketLogError::Provider)
    }

    async fn list_syncable_buckets(
        &self,
    ) -> Result<Vec<Uuid>, zim_protocol::log::BucketLogError<Self::Error>> {
        // Single query: buckets that are explicitly active OR have no status row (backward compat)
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT bl.bucket_id \
             FROM bucket_log bl \
             LEFT JOIN bucket_status bs ON bl.bucket_id = bs.bucket_id \
             WHERE bs.status IS NULL OR bs.status = 'active' \
             ORDER BY bl.bucket_id",
        )
        .fetch_all(&**self)
        .await
        .map_err(zim_protocol::log::BucketLogError::Provider)?;

        Ok(rows
            .into_iter()
            .map(|r| Uuid::parse_str(&r.0).expect("invalid bucket_id UUID in database"))
            .collect())
    }
}
